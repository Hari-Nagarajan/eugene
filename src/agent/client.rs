use rig::providers::openai;

const DEFAULT_BASE_URL: &str = "https://api.minimax.chat/v1";
const DEFAULT_MODEL: &str = "MiniMax-M2.5";

/// Create a MiniMax client using rig's OpenAI CompletionsClient with custom base URL.
///
/// MiniMax M2.5 uses an OpenAI-compatible Chat Completions API, so we use
/// `CompletionsClient` (NOT the default Responses API client).
///
/// Reads configuration from environment variables:
/// - `MINIMAX_API_KEY` (required) - API key for MiniMax. Panics if not set.
/// - `MINIMAX_BASE_URL` (optional) - defaults to `https://api.minimax.chat/v1`
/// - `MINIMAX_MODEL` (optional) - defaults to `MiniMax-M2.5`
///
/// Returns the client and model name as a tuple.
pub fn create_minimax_client() -> (openai::CompletionsClient, String) {
    let api_key = std::env::var("MINIMAX_API_KEY")
        .expect("MINIMAX_API_KEY must be set. Get your key from the MiniMax dashboard -> API keys.");

    let base_url = std::env::var("MINIMAX_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

    let model_name = std::env::var("MINIMAX_MODEL")
        .unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    let client = openai::CompletionsClient::builder()
        .api_key(&api_key)
        .base_url(&base_url)
        .build()
        .expect("Failed to build MiniMax CompletionsClient");

    (client, model_name)
}
