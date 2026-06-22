use airframe_db::{retry, run_with_timeout, AirframeDbError, Result, RetryPolicy};
use std::time::Duration;

fn main() -> Result<()> {
    // Demonstrate retry(): transient errors then success
    let mut attempts = 0u32;
    let val = retry(
        RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(5),
            jitter_frac: 0.0,
        },
        |_| {
            attempts += 1;
            if attempts < 3 {
                return Err(AirframeDbError::Connection("temporary".into()));
            }
            Ok(attempts)
        },
    )?;
    println!("retry_timeout: retry succeeded after {} attempts", val);

    // Demonstrate run_with_timeout(): this will time out
    let too_slow = run_with_timeout(Duration::from_millis(10), || {
        std::thread::sleep(Duration::from_millis(25));
        Ok::<_, AirframeDbError>(())
    });
    match too_slow {
        Ok(_) => println!("unexpected success"),
        Err(AirframeDbError::Timeout) => println!("timeout helper returned Timeout as expected"),
        Err(e) => println!("unexpected error: {}", e),
    }

    Ok(())
}
