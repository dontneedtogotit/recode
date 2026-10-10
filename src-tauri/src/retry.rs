use std::time::Duration;

const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(200);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    Exponential,
    Fixed,
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        BackoffStrategy::Exponential
    }
}

#[derive(Debug, Clone)]
pub struct RetryOptions {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub strategy: BackoffStrategy,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            strategy: BackoffStrategy::Exponential,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryError {
    pub attempts: u32,
    pub last_error: String,
}

impl RetryError {
    pub fn new(attempts: u32, last_error: impl Into<String>) -> Self {
        Self {
            attempts,
            last_error: last_error.into(),
        }
    }
}

pub struct Retry;

impl Retry {
    pub fn execute<F, T>(options: RetryOptions, mut f: F) -> Result<T, RetryError>
    where
        F: FnMut() -> Result<T, RetryError>,
    {
        let mut last_error = None;
        for attempt in 0..=options.max_retries {
            match f() {
                Ok(value) => return Ok(value),
                Err(error) => {
                    last_error = Some(error);
                    if attempt == options.max_retries {
                        break;
                    }
                    let delay = Self::delay_for(options.strategy, attempt, options.base_delay, options.max_delay);
                    std::thread::sleep(delay);
                }
            }
        }

        Err(RetryError::new(
            options.max_retries + 1,
            last_error
                .map(|error| error.last_error)
                .unwrap_or_else(|| "unknown retry failure".into()),
        ))
    }

    fn delay_for(strategy: BackoffStrategy, attempt: u32, base_delay: Duration, max_delay: Duration) -> Duration {
        let delay = match strategy {
            BackoffStrategy::Exponential => base_delay.saturating_mul(2u32.saturating_pow(attempt)),
            BackoffStrategy::Fixed => base_delay,
        };
        delay.min(max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn succeeds_on_first_attempt() {
        let result = Retry::execute(RetryOptions::default(), || Ok(1));
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn retries_until_success() {
        let mut attempts = 0;
        let result = Retry::execute(RetryOptions {
            max_retries: 2,
            ..RetryOptions::default()
        }, || {
            attempts += 1;
            if attempts < 3 {
                Err(RetryError::new(1, "transient"))
            } else {
                Ok(attempts)
            }
        });
        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn returns_error_after_max_retries() {
        let result: Result<(), RetryError> = Retry::execute(
            RetryOptions {
                max_retries: 1,
                ..RetryOptions::default()
            },
            || Err(RetryError::new(1, "fatal")),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().attempts, 2);
    }
}