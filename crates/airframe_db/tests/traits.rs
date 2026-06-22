use airframe_db::*;

#[derive(Clone, Default)]
struct MockPool;
struct MockConn;

impl DbConnection for MockConn {
    fn ping(&self) -> Result<()> {
        Ok(())
    }
}

impl DbPool for MockPool {
    type Conn = MockConn;
    fn get(&self) -> Result<Self::Conn> {
        Ok(MockConn)
    }
}

#[test]
fn can_get_and_ping() {
    let p = MockPool;
    let c = p.get().unwrap();
    c.ping().unwrap();
}
