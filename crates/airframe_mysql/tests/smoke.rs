#[test]
fn ping_and_name() {
    assert!(airframe_mysql::ping());
    assert!(!airframe_mysql::CRATE.is_empty());
}
