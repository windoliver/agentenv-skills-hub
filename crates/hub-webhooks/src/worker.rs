use std::time::Duration;

pub fn retry_delay(attempts: u32) -> Duration {
    Duration::from_secs(2_u64.pow(attempts.min(6)))
}
