use airframe_db::config::parse_connection_string;

#[test]
fn mysql_dsn_parsing_basic() {
    let cs = parse_connection_string("mysql://user:pass@localhost:3306/mydb?ssl=off").unwrap();
    assert_eq!(cs.scheme, "mysql");
    assert_eq!(cs.user.as_deref(), Some("user"));
    assert_eq!(cs.password.as_deref(), Some("pass"));
    assert_eq!(cs.host.as_deref(), Some("localhost"));
    assert_eq!(cs.port, Some(3306));
    assert_eq!(cs.database.as_deref(), Some("mydb"));
    assert_eq!(cs.params.get("ssl").map(String::as_str), Some("off"));
}

#[test]
fn mysql_dsn_parsing_percent_decoded() {
    let cs = parse_connection_string("mysql://us%65r:p%61ss@h%6Fst/db%32").unwrap();
    assert_eq!(cs.user.as_deref(), Some("user"));
    assert_eq!(cs.password.as_deref(), Some("pass"));
    assert_eq!(cs.host.as_deref(), Some("host"));
    assert_eq!(cs.database.as_deref(), Some("db2"));
}

#[test]
fn mysql_dsn_parsing_invalid() {
    assert!(parse_connection_string("mysql://host:badport/db").is_err());
}
