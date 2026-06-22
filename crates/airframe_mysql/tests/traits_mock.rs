use airframe_db::connection::{DbConnection, SqlExec, SqlParam, SqlRows, SqlValue};
use airframe_db::Result;

#[derive(Clone, Default)]
struct MockConn;

impl DbConnection for MockConn {
    fn ping(&self) -> Result<()> {
        Ok(())
    }
}

impl SqlExec for MockConn {
    fn execute(&self, sql: &str, _params: &[SqlParam]) -> Result<u64> {
        // Return 1 row affected for INSERT/UPDATE-like statements, else 0
        let s = sql.trim().to_ascii_lowercase();
        let affected =
            if s.starts_with("insert") || s.starts_with("update") || s.starts_with("delete") {
                1
            } else {
                0
            };
        Ok(affected)
    }

    fn query(&self, sql: &str, _params: &[SqlParam]) -> Result<SqlRows> {
        // Return a fixed rowset for SELECT COUNT(*)
        let s = sql.trim().to_ascii_lowercase();
        if s.contains("count(*)") {
            Ok(SqlRows {
                columns: vec!["count".into()],
                rows: vec![vec![SqlValue::U64(42)]],
            })
        } else {
            Ok(SqlRows {
                columns: vec![],
                rows: vec![],
            })
        }
    }
}

#[test]
fn trait_compliance_with_mock() {
    let c = MockConn;
    c.ping().unwrap();
    let n = c.execute("INSERT INTO t VALUES (1)", &[]).unwrap();
    assert_eq!(n, 1);
    let rows = c.query("SELECT COUNT(*) FROM t", &[]).unwrap();
    assert_eq!(rows.columns, vec!["count".to_string()]);
    assert_eq!(rows.rows.len(), 1);
}
