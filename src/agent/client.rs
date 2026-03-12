use rig::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage, Usage,
};
use rig::providers::{anthropic, openrouter};
use rig::streaming::StreamingCompletionResponse;
use serde::{Deserialize, Serialize};

use crate::config::Config;

const DEFAULT_BASE_URL: &str = "https://api.minimax.io/anthropic";
const DEFAULT_MODEL: &str = "MiniMax-M2.5";

// ---------- Response wrappers ----------

/// Unified response type wrapping provider-specific completion responses.
#[derive(Serialize, Deserialize)]
pub enum AnyResponse {
    Anthropic(anthropic::completion::CompletionResponse),
    OpenRouter(openrouter::completion::CompletionResponse),
}

/// Unified streaming response type wrapping provider-specific streaming responses.
#[derive(Clone, Serialize, Deserialize)]
pub enum AnyStreamingResponse {
    Anthropic(anthropic::streaming::StreamingCompletionResponse),
    OpenRouter(openrouter::streaming::StreamingCompletionResponse),
}

impl GetTokenUsage for AnyStreamingResponse {
    fn token_usage(&self) -> Option<Usage> {
        match self {
            Self::Anthropic(r) => r.token_usage(),
            Self::OpenRouter(r) => r.token_usage(),
        }
    }
}

// ---------- AnyModel enum ----------

/// Provider-agnostic completion model wrapping either MiniMax (via Anthropic client)
/// or OpenRouter behind a single `CompletionModel` implementation.
///
/// Constructed exclusively through [`create_client()`].
#[derive(Clone)]
pub enum AnyModel {
    Anthropic(anthropic::completion::CompletionModel),
    OpenRouter(openrouter::CompletionModel),
}

impl std::fmt::Debug for AnyModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anthropic(_) => write!(f, "AnyModel::Anthropic(...)"),
            Self::OpenRouter(_) => write!(f, "AnyModel::OpenRouter(...)"),
        }
    }
}

impl CompletionModel for AnyModel {
    type Response = AnyResponse;
    type StreamingResponse = AnyStreamingResponse;
    type Client = ();

    fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
        panic!("AnyModel cannot be constructed via make() -- use create_client()")
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<AnyResponse>, CompletionError> {
        match self {
            Self::Anthropic(m) => {
                let resp = m.completion(request).await?;
                Ok(CompletionResponse {
                    choice: resp.choice,
                    usage: resp.usage,
                    raw_response: AnyResponse::Anthropic(resp.raw_response),
                    message_id: resp.message_id,
                })
            }
            Self::OpenRouter(m) => {
                let resp = m.completion(request).await?;
                Ok(CompletionResponse {
                    choice: resp.choice,
                    usage: resp.usage,
                    raw_response: AnyResponse::OpenRouter(resp.raw_response),
                    message_id: resp.message_id,
                })
            }
        }
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<AnyStreamingResponse>, CompletionError> {
        unimplemented!("AnyModel does not support streaming")
    }
}

// ---------- Factory ----------

/// Create an LLM completion model from the given configuration.
///
/// Reads `config.provider`, `config.minimax_api_key`, `config.model`, and
/// `config.base_url` to construct the appropriate provider client and return
/// an [`AnyModel`] that implements `CompletionModel`.
///
/// # Provider behavior
///
/// - `"minimax"` (or `None` with an API key present): Uses rig's Anthropic client
///   pointed at MiniMax's API. Defaults to model `MiniMax-M2.5` and base URL
///   `https://api.minimax.io/anthropic`.
///
/// - `"openrouter"`: Uses rig's native OpenRouter client. Requires both an API key
///   and a model name (no default model for OpenRouter).
///
/// # Errors
///
/// Returns an error if:
/// - The API key is missing
/// - OpenRouter is selected but no model is configured
/// - An unknown provider name is given
/// - The underlying client fails to build
pub fn create_client(config: &Config) -> Result<AnyModel, anyhow::Error> {
    match config.provider.as_deref() {
        Some("minimax") | None if config.minimax_api_key.is_some() => {
            let api_key = config.minimax_api_key.as_deref().unwrap();
            let base_url = config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
            let model_name = config.model.as_deref().unwrap_or(DEFAULT_MODEL);

            let client = anthropic::Client::builder()
                .api_key(api_key)
                .base_url(base_url)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build MiniMax client: {e}"))?;

            let model = rig::prelude::CompletionClient::completion_model(&client, model_name);
            Ok(AnyModel::Anthropic(model))
        }
        Some("minimax") => {
            Err(anyhow::anyhow!(
                "No API key configured for MiniMax. Set MINIMAX_API_KEY or run `eugene init`."
            ))
        }
        Some("openrouter") => {
            let api_key = config.minimax_api_key.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "No API key configured for OpenRouter. Run `eugene init` or set api_key in config."
                )
            })?;
            let model_name = config.model.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenRouter requires a model name. Run `eugene init` or pass --model."
                )
            })?;

            let client = openrouter::Client::builder()
                .api_key(api_key)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build OpenRouter client: {e}"))?;

            let model = rig::prelude::CompletionClient::completion_model(&client, model_name);
            Ok(AnyModel::OpenRouter(model))
        }
        None => {
            Err(anyhow::anyhow!(
                "No provider configured. Run `eugene init` to set up your LLM provider."
            ))
        }
        Some(other) => {
            Err(anyhow::anyhow!(
                "Unknown provider '{other}'. Supported: minimax, openrouter"
            ))
        }
    }
}

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
