use anyhow::Result;
use std::future::Future;
use tokio::time::{Duration, sleep};
use tracing::{debug, warn};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries (will be multiplied by 2^attempt)
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryConfig {
    pub fn new(max_retries: u32, base_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay: Duration::from_millis(base_delay_ms),
            max_delay: Duration::from_secs(60),
        }
    }

    /// Calculate delay for a given attempt (exponential backoff)
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.base_delay * 2u32.saturating_pow(attempt);
        std::cmp::min(delay, self.max_delay)
    }
}

/// Retry a fallible async operation with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        operation = operation_name,
                        attempt, "operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);

                if attempt < config.max_retries {
                    let delay = config.delay_for_attempt(attempt);
                    warn!(
                        operation = operation_name,
                        attempt,
                        max_retries = config.max_retries,
                        delay_ms = delay.as_millis() as u64,
                        error = %last_error.as_ref().unwrap(),
                        "operation failed, retrying"
                    );
                    sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Check if an error is retryable (transient errors)
pub fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Network/connection errors
    if error_str.contains("connection")
        || error_str.contains("timeout")
        || error_str.contains("timed out")
        || error_str.contains("network")
    {
        return true;
    }

    // Rate limiting
    if error_str.contains("rate limit")
        || error_str.contains("too many requests")
        || error_str.contains("429")
    {
        return true;
    }

    // Server errors (5xx)
    if error_str.contains("500")
        || error_str.contains("502")
        || error_str.contains("503")
        || error_str.contains("504")
        || error_str.contains("internal server error")
        || error_str.contains("bad gateway")
        || error_str.contains("service unavailable")
    {
        return true;
    }

    // API overloaded
    if error_str.contains("overloaded") || error_str.contains("capacity") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::new(3, 1000);

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(4000));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(8000));
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig {
            max_retries: 10,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };

        // 2^10 seconds would be 1024 seconds, but should be capped at 10
        assert_eq!(config.delay_for_attempt(10), Duration::from_secs(10));
    }

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(&anyhow::anyhow!("connection refused")));
        assert!(is_retryable_error(&anyhow::anyhow!("request timed out")));
        assert!(is_retryable_error(&anyhow::anyhow!("rate limit exceeded")));
        assert!(is_retryable_error(&anyhow::anyhow!(
            "503 Service Unavailable"
        )));
        assert!(is_retryable_error(&anyhow::anyhow!("API overloaded")));

        assert!(!is_retryable_error(&anyhow::anyhow!("invalid api key")));
        assert!(!is_retryable_error(&anyhow::anyhow!("model not found")));
    }
}
