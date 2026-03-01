//! Mock completion model for testing the agent loop without a live API.
//!
//! Provides `MockCompletionModel` which implements rig's `CompletionModel` trait
//! with a queue of canned responses. Each call to `completion()` pops the next
//! response, allowing deterministic testing of multi-turn tool-call flows.

use rig::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage, Usage,
};
use rig::message::AssistantContent;
use rig::streaming::StreamingCompletionResponse;
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Placeholder raw response type for mock completions.
#[derive(Clone, Serialize, Deserialize)]
pub struct MockRawResponse;

/// Placeholder streaming response type (streaming not supported in mock).
#[derive(Clone, Serialize, Deserialize)]
pub struct MockStreamingResponse;

impl GetTokenUsage for MockStreamingResponse {
    fn token_usage(&self) -> Option<Usage> {
        None
    }
}

/// A mock completion model that returns canned responses in order.
///
/// Each call to `completion()` removes and returns the first response from the queue.
/// This allows testing multi-turn agent flows deterministically:
///
/// ```rust,ignore
/// let mock = MockCompletionModel::new(vec![
///     OneOrMany::one(AssistantContent::tool_call("call_001", "run_command", json!({"command": "nmap -sS 10.0.0.1"}))),
///     OneOrMany::one(AssistantContent::tool_call("call_002", "log_discovery", json!({"finding_type": "port_scan", "host": "10.0.0.1", "data": "Open ports: 22, 80"}))),
///     OneOrMany::one(AssistantContent::text("Scan complete.")),
/// ]);
/// ```
#[derive(Clone)]
pub struct MockCompletionModel {
    responses: Arc<Mutex<Vec<OneOrMany<AssistantContent>>>>,
}

impl MockCompletionModel {
    /// Create a new mock model with a queue of canned responses.
    ///
    /// Responses are consumed in order -- one per `completion()` call.
    /// Panics if the queue is exhausted before the agent loop completes.
    pub fn new(responses: Vec<OneOrMany<AssistantContent>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
        }
    }
}

impl CompletionModel for MockCompletionModel {
    type Response = MockRawResponse;
    type StreamingResponse = MockStreamingResponse;
    type Client = ();

    fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
        panic!("MockCompletionModel cannot be constructed via make() -- use MockCompletionModel::new() directly")
    }

    async fn completion(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse<MockRawResponse>, CompletionError> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            panic!("MockCompletionModel: response queue exhausted -- add more canned responses");
        }
        let choice = responses.remove(0);
        Ok(CompletionResponse {
            choice,
            usage: Usage::new(),
            raw_response: MockRawResponse,
            message_id: None,
        })
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<MockStreamingResponse>, CompletionError> {
        unimplemented!("MockCompletionModel does not support streaming")
    }
}
