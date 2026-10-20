//! D11, SEC-1: exercise the real backend only when explicitly requested.
//! All data belongs to a fresh test service; no gitcroc credential is read.

#[test]
#[ignore = "needs an unlocked OS keychain; Windows/macOS CI runs this test explicitly"]
fn d11_os_keychain_round_trip() {
    let service = format!("gitcroc-test-{}", uuid::Uuid::new_v4());
    assert_ne!(service, gitcroc_app::platform::KEYCHAIN_SERVICE);
    let entry = keyring::Entry::new(&service, "round-trip").unwrap();
    struct Cleanup(keyring::Entry);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.delete_credential();
        }
    }
    let entry = Cleanup(entry);
    assert!(matches!(
        entry.0.get_password(),
        Err(keyring::Error::NoEntry)
    ));
    // Made-up test data generated here; never a provider key or a user entry.
    let value = uuid::Uuid::new_v4().to_string();
    entry.0.set_password(&value).unwrap();
    assert!(entry.0.get_password().is_ok_and(|read| read == value));
    entry.0.delete_credential().unwrap();
    assert!(matches!(
        entry.0.get_password(),
        Err(keyring::Error::NoEntry)
    ));
}
