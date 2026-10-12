//! Regenerates `ui/src/ipc/bindings.ts`; run through `just ipc`.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(gitcroc_app::ipc::BINDINGS_PATH);
    match gitcroc_app::ipc::export_bindings(&path) {
        Ok(()) => {
            println!("wrote {}", gitcroc_app::ipc::BINDINGS_PATH);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot export IPC bindings: {error}");
            ExitCode::FAILURE
        }
    }
}
