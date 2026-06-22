#[cfg(feature = "driver")]
#[test]
fn feature_on_compiles_and_pool_gets_conn() {
    use airframe_db::connection::DbPool;
    let pool = airframe_mysql::MySqlPool::new("mysql://user:pass@localhost:3306/db");
    let _conn = pool.get().unwrap();
}
