#[test]
fn ping_and_name() {
    assert!(airframe_sqlite::ping());
    assert!(!airframe_sqlite::CRATE.is_empty());
}
