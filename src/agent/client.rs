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
