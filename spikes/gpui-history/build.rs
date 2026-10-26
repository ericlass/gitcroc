// Some distributions ship libxkbcommon-x11.so.0 without the unversioned
// development symlink; link against the runtime library so no -dev
// package is needed.
fn main() {
    #[cfg(target_os = "linux")]
    {
        let runtime = std::path::Path::new("/usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0");
        let Some(out) = std::env::var_os("OUT_DIR") else { return };
        let link = std::path::Path::new(&out).join("libxkbcommon-x11.so");
        if runtime.exists() && !link.exists() {
            let _ = std::os::unix::fs::symlink(runtime, &link);
        }
        println!("cargo:rustc-link-search=native={}", std::path::Path::new(&out).display());
    }
}
