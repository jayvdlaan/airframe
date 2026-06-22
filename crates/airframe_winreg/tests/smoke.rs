#[test]
fn ping_and_name() {
    assert!(airframe_winreg::ping());
    assert!(!airframe_winreg::CRATE.is_empty());
}
