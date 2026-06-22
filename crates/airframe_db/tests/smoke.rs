#[test]
fn ping_and_name() {
    assert!(airframe_db::ping());
    assert!(!airframe_db::CRATE.is_empty());
}
