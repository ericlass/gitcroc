//! `gitcroc-ai` keeps its own copy of the conflict marker rule, since it
//! depends on no other crate (ARCH-5). This crate depends on both.

#[test]
fn ai_9_marker_rule_agrees_with_the_editors() {
    for text in [
        "",
        "a\n",
        "<<<<<<<\n",
        "<<<<<<< HEAD\n",
        "a\n======= x\n",
        "a\n|||||||",
        ">>>>>>> theirs\r\n",
        "a\r\nb\r\n=======\r\n",
        "<<<<<<<<\n",
        " =======\n",
        "<<<<<<\n",
        ">>>>>>>x\n",
        ">>>>>>>\tx\n",
        "=-=-=-=\n",
        "// =======\n",
        "a\nb\n=======\n",
        "a\n\n\n<<<<<<<",
        "=======\r",
        "a\rb\n=======\n",
        "\n\n",
    ] {
        assert_eq!(
            gitcroc_ai::first_marker_line(text),
            gitcroc_git::first_marker_line(text),
            "{text:?}"
        );
    }
}
