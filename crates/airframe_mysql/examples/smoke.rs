fn main() {
    println!(
        "{}: ping={} ",
        airframe_mysql::CRATE,
        airframe_mysql::ping()
    );
}
