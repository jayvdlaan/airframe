#[test]
fn ping_and_name() {
    assert!(airframe_redis::ping());
    assert!(!airframe_redis::CRATE.is_empty());
}
