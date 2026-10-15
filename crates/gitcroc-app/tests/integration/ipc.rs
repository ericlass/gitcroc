//! The IPC surface: every command is async (ARCH-7) and the committed
//! TypeScript bindings match the Rust code.

use std::fs;
use std::path::Path;

use gitcroc_app::ipc;

#[test]
fn arch_7_commands_are_async() {
    let commands = ipc::command_signatures();
    let names: Vec<&str> = commands.iter().map(|f| f.name()).collect();
    assert!(names.contains(&"app_info"), "commands: {names:?}");
    for command in &commands {
        assert!(
            command.asyncness(),
            "ARCH-7: command `{}` is not async; a sync command runs on the main thread and blocks the UI",
            command.name()
        );
    }
}

#[test]
fn ipc_bindings_are_up_to_date() {
    let committed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(ipc::BINDINGS_PATH);
    let fresh = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bindings.ts");
    ipc::export_bindings(&fresh).unwrap();

    // .gitattributes checks out LF everywhere; normalize anyway so a stray
    // CRLF checkout fails on content, not line endings.
    let read = |path: &Path| fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    assert!(
        read(&committed) == read(&fresh),
        "{} is out of date: run `just ipc` and commit the result",
        ipc::BINDINGS_PATH
    );
}
