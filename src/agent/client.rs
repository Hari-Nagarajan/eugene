use rig::providers::anthropic;

const DEFAULT_BASE_URL: &str = "https://api.minimax.io/anthropic";
const DEFAULT_MODEL: &str = "MiniMax-M2.5";

/// Create a MiniMax client using rig's Anthropic client with custom base URL.
///
/// Reads configuration from environment variables:
/// - `MINIMAX_API_KEY` (required) - API key for MiniMax.
/// - `MINIMAX_BASE_URL` (optional) - defaults to `https://api.minimax.io/anthropic`
/// - `MINIMAX_MODEL` (optional) - defaults to `MiniMax-M2.5`
///
/// Returns the client and model name as a tuple.
///
/// # Errors
///
/// Returns an error if `MINIMAX_API_KEY` is not set or the client fails to build.
#[deprecated(note = "Use create_client(&Config) instead -- will be removed in plan 02")]
pub fn create_minimax_client() -> Result<(anthropic::Client, String), anyhow::Error> {
    let api_key = std::env::var("MINIMAX_API_KEY")
        .map_err(|_| anyhow::anyhow!("MINIMAX_API_KEY not set. Get your key from the MiniMax dashboard -> API keys."))?;

    let base_url = std::env::var("MINIMAX_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

    let model_name = std::env::var("MINIMAX_MODEL")
        .unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    let client = anthropic::Client::builder()
        .api_key(&api_key)
        .base_url(&base_url)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build MiniMax client: {e}"))?;

    Ok((client, model_name))
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    // -- create_client tests (RED: function doesn't exist yet) --

    #[test]
    fn test_create_client_minimax_with_valid_api_key() {
        let config = Config {
            provider: Some("minimax".to_string()),
            minimax_api_key: Some("test-key-123".to_string()),
            ..Config::default()
        };
        let model = super::create_client(&config);
        assert!(model.is_ok(), "create_client should succeed with valid minimax config");
    }

    #[test]
    fn test_create_client_openrouter_with_valid_config() {
        let config = Config {
            provider: Some("openrouter".to_string()),
            minimax_api_key: Some("or-test-key".to_string()),
            model: Some("anthropic/claude-3.5-sonnet".to_string()),
            ..Config::default()
        };
        let model = super::create_client(&config);
        assert!(model.is_ok(), "create_client should succeed with valid openrouter config");
    }

    #[test]
    fn test_create_client_minimax_missing_api_key() {
        let config = Config {
            provider: Some("minimax".to_string()),
            minimax_api_key: None,
            ..Config::default()
        };
        let result = super::create_client(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("API key"), "Error should mention API key, got: {err}");
    }

    #[test]
    fn test_create_client_openrouter_missing_model() {
        let config = Config {
            provider: Some("openrouter".to_string()),
            minimax_api_key: Some("or-key".to_string()),
            model: None,
            ..Config::default()
        };
        let result = super::create_client(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("model"), "Error should mention model, got: {err}");
    }

    #[test]
    fn test_create_client_unknown_provider() {
        let config = Config {
            provider: Some("google".to_string()),
            minimax_api_key: Some("key".to_string()),
            ..Config::default()
        };
        let result = super::create_client(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown provider"), "Error should mention unknown provider, got: {err}");
    }

    #[test]
    fn test_create_client_no_provider_no_key() {
        let config = Config {
            provider: None,
            minimax_api_key: None,
            ..Config::default()
        };
        let result = super::create_client(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("No provider") || err.contains("API key") || err.contains("eugene init"),
            "Error should guide user to configure, got: {err}"
        );
    }
}
