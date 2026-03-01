use rig::providers::anthropic;

const DEFAULT_BASE_URL: &str = "https://api.minimax.io/anthropic";
const DEFAULT_MODEL: &str = "MiniMax-M2.5";

/// Create a MiniMax client using rig's Anthropic client with custom base URL.
///
/// Reads configuration from environment variables:
/// - `MINIMAX_API_KEY` (required) - API key for MiniMax. Panics if not set.
/// - `MINIMAX_BASE_URL` (optional) - defaults to `https://api.minimax.io/anthropic`
/// - `MINIMAX_MODEL` (optional) - defaults to `MiniMax-M2.5`
///
/// Returns the client and model name as a tuple.
pub fn create_minimax_client() -> (anthropic::Client, String) {
    let api_key = std::env::var("MINIMAX_API_KEY")
        .expect("MINIMAX_API_KEY must be set. Get your key from the MiniMax dashboard -> API keys.");

    let base_url = std::env::var("MINIMAX_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

    let model_name = std::env::var("MINIMAX_MODEL")
        .unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    let client = anthropic::Client::builder()
        .api_key(&api_key)
        .base_url(&base_url)
        .build()
        .expect("Failed to build MiniMax client");

    (client, model_name)
}
