use eugene::agent::client::create_minimax_client;
use eugene::agent::create_agent;
use eugene::config::Config;
use eugene::memory::{init_schema, open_memory_store};
use rig::prelude::CompletionClient;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("Eugene - Autonomous Recon Agent\n");

    // Read task from CLI arg, or use default
    let task = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scan the local network with arp-scan".to_string());

    // Initialize memory store and schema
    let memory = open_memory_store("eugene.db").await?;
    init_schema(&memory).await?;

    // Create config
    let config = Arc::new(Config::default());

    // Create MiniMax client (panics if MINIMAX_API_KEY not set)
    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    // Build agent with all recon tools
    let agent = create_agent(model, config, memory);

    // Execute task
    println!("Task: {task}\n");
    let result: String = eugene::agent::run_recon_task(&agent, &task).await?;
    println!("{result}");

    Ok(())
}
