//! UI-22, PERF-1: the actual history's bar scrolls its virtualized rows.
use crate::history::{
    add, click, repo, repo_node, repos, tree_row, wait_for_element, wait_until, with_history,
};
use crate::support::{Fixture, main_window};
use gitcroc_app::app::EVENT_CAPACITY;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{TestAppContext, point, px};

#[gpui::test]
fn ui_22_history_scrollbar_moves_the_virtual_list_without_a_wheel(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    repo(&made, "api")
        .empty_commits(&(0..400).map(|n| format!("Commit {n}")).collect::<Vec<_>>())
        .unwrap();
    let added = add(&fixture, &made);
    let (mut cx, view) = main_window(cx, &fixture);
    let row = tree_row(&repo_node(&added, "api"));
    wait_for_element(&mut cx, row.clone());
    click(&mut cx, row);
    wait_until(&mut cx, |cx| {
        with_history(cx, &view, |history| {
            !history.loading() && history.rows().len() == 401
        })
    });
    cx.update(|window, cx| {
        assert_eq!(
            gpui_component::Theme::global(cx).scrollbar_mode,
            gpui_component::scroll::ScrollbarMode::Always
        );
        window.render_frame(cx);
        let bounds = window.find("history-list").bounds();
        window.click_at(
            "history-list",
            point(bounds.size.width - px(4.), bounds.size.height * 0.8),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            (100..401).any(|i| window
                .try_find(gpui_kit::ElementId::Name(format!("history-row-{i}").into()))
                .is_some()),
            "scrollbar track did not move the list"
        );
        assert!(
            window.try_find("history-row-0").is_none(),
            "the list stayed at its beginning"
        );
    });
}

struct ScrollContent;
impl gpui_kit::Render for ScrollContent {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gitcroc_ui::scrollbars::ScrollAreaExt;
        use gpui_kit::{
            InteractiveElement, IntoElement, ParentElement, Styled, TestSupportExt, div,
        };
        div()
            .id("ordinary-scroll")
            .test_support()
            .w(px(300.))
            .h(px(200.))
            .visible_y_scroll()
            .children((0..100).map(|n| {
                div()
                    .id(gpui_kit::ElementId::Name(
                        format!("ordinary-row-{n}").into(),
                    ))
                    .test_support()
                    .h(px(20.))
                    .child(format!("Line {n}"))
                    .into_any_element()
            }))
    }
}

#[gpui::test]
fn ui_22_ordinary_scrollbar_preserves_the_container_and_stays_at_its_viewport(
    cx: &mut TestAppContext,
) {
    use gpui_kit::AppContext;
    let (mut cx, _) = crate::support::open(cx, |_, cx| cx.new(|_| ScrollContent));
    let first = cx.update(|window, _| window.find("ordinary-row-0").bounds().top());
    cx.update(|window, cx| window.click_at("ordinary-scroll", point(px(296.), px(160.)), cx));
    cx.run_until_parked();
    let down = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("ordinary-row-0").bounds().top()
    });
    assert!(down < first);
    cx.update(|window, cx| window.click_at("ordinary-scroll", point(px(296.), px(20.)), cx));
    cx.run_until_parked();
    let up = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("ordinary-row-0").bounds().top()
    });
    assert!(
        up > down,
        "the scrollbar moved with its content instead of staying at the viewport"
    );
}
