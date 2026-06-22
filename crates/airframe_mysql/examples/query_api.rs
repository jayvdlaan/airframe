use airframe_db::connection::{DbConnection, SqlExec, SqlParam, SqlRows, SqlValue};

#[derive(Clone, Default)]
struct MockConn;

impl DbConnection for MockConn {
    fn ping(&self) -> airframe_db::Result<()> {
        Ok(())
    }
}

impl SqlExec for MockConn {
    fn execute(&self, sql: &str, _params: &[SqlParam]) -> airframe_db::Result<u64> {
        println!("EXEC: {}", sql);
        Ok(1)
    }
    fn query(&self, sql: &str, _params: &[SqlParam]) -> airframe_db::Result<SqlRows> {
        println!("QUERY: {}", sql);
        Ok(SqlRows {
            columns: vec!["id".into(), "name".into()],
            rows: vec![vec![SqlValue::U64(1), SqlValue::Str("alpha".into())]],
        })
    }
}

// Run with:
// cargo run -q -p airframe_mysql --example query_api
fn main() -> airframe_db::Result<()> {
    let conn = MockConn;
    conn.ping()?;
    let _ = conn.execute("CREATE TABLE t (id INT PRIMARY KEY, name TEXT)", &[])?;
    let _ = conn.execute("INSERT INTO t (id, name) VALUES (1, 'alpha')", &[])?;
    let rows = conn.query("SELECT id, name FROM t", &[])?;
    println!("columns={:?} rows={:?}", rows.columns, rows.rows);
    Ok(())
}
