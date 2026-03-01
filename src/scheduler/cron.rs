use croner::Cron;
use std::str::FromStr;

/// Validate a 5-field cron expression.
///
/// Returns Ok(()) if the expression is valid, or Err with a human-readable message.
pub fn validate_cron(expr: &str) -> Result<(), String> {
    Cron::from_str(expr).map(|_| ()).map_err(|e| e.to_string())
}

/// Compute the next occurrence of a cron expression as a unix timestamp (i64).
///
/// Returns Err if the expression is invalid or no next occurrence can be found.
pub fn next_occurrence(expr: &str) -> Result<i64, String> {
    let cron = Cron::from_str(expr).map_err(|e| e.to_string())?;
    let next = cron
        .find_next_occurrence(&chrono::Utc::now(), false)
        .map_err(|e| e.to_string())?;
    Ok(next.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_valid() {
        assert!(validate_cron("0 */6 * * *").is_ok());
        assert!(validate_cron("* * * * *").is_ok());
        assert!(validate_cron("0 0 1 1 *").is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        assert!(validate_cron("bad cron").is_err());
        assert!(validate_cron("").is_err());
    }

    #[test]
    fn test_next_occurrence_returns_future_timestamp() {
        let ts = next_occurrence("* * * * *").unwrap();
        let now = chrono::Utc::now().timestamp();
        assert!(ts >= now, "Next occurrence should be in the future");
    }

    #[test]
    fn test_next_occurrence_invalid_cron() {
        assert!(next_occurrence("bad").is_err());
    }
}
