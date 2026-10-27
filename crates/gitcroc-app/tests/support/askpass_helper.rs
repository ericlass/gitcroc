//! Git's askpass helper for the askpass tests (D10): what the app's binary
//! does first when git starts it as its helper.

use gitcroc_app::askpass::helper;
use gitcroc_app::platform::PlatformSockets;

fn main() {
    std::process::exit(helper::<PlatformSockets>().unwrap_or(1));
}
