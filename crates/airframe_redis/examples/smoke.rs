fn main() {
    println!(
        "{}: ping={} ",
        airframe_redis::CRATE,
        airframe_redis::ping()
    );
}
