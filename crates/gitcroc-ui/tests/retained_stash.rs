#![allow(clippy::unwrap_used, clippy::expect_used)]
use gitcroc_core::{JobId, Outcome, RepoId, StashRef, WorkspaceId};
use gitcroc_ui::retained_stash::RetainedStashes;
#[test]
fn arch_9_conflicting_pop_offer_is_keyed_by_workspace_and_repo() {
    let mut kept = RetainedStashes::default();
    let w = WorkspaceId("w".into());
    let r = RepoId("r".into());
    kept.pop(
        JobId(1),
        w.clone(),
        r.clone(),
        StashRef {
            index: 0,
            hash: "hash".into(),
        },
    );
    kept.ended(JobId(1), Some(&Outcome::Conflict));
    assert_eq!(kept.offers(&w).count(), 1);
    assert_eq!(kept.offers(&WorkspaceId("other".into())).count(), 0);
    kept.forget(&w, &r);
    assert_eq!(kept.offers(&w).count(), 0);
}
#[test]
fn ops_7_pop_end_before_request_reply_still_offers_kept_stash() {
    let mut kept = RetainedStashes::default();
    let w = WorkspaceId("w".into());
    let r = RepoId("r".into());
    kept.ended(JobId(1), Some(&Outcome::Conflict));
    kept.pop(
        JobId(1),
        w.clone(),
        r.clone(),
        StashRef {
            index: 0,
            hash: "hash".into(),
        },
    );
    assert_eq!(kept.offers(&w).next().unwrap().1.hash, "hash");
    kept.pop(
        JobId(2),
        w.clone(),
        r,
        StashRef {
            index: 0,
            hash: "new".into(),
        },
    );
    kept.ended(JobId(2), Some(&Outcome::Succeeded));
    assert_eq!(kept.offers(&w).count(), 1);
}
