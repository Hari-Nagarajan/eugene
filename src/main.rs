use eugene::agent::client::create_minimax_client;
use eugene::agent::create_agent;
use eugene::config::Config;
use eugene::memory::{init_schema, open_memory_store};
use rig::prelude::CompletionClient;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("Eugene - Autonomous Recon Agent\n");

    // Initialize memory store and schema
    let memory = open_memory_store("eugene.db").await?;
    init_schema(&memory).await?;

    // Create config
    let config = Arc::new(Config::default());

    // Create MiniMax client (panics if MINIMAX_API_KEY not set)
    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    // Parse CLI args for mode selection
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--campaign" {
        // Campaign mode: multi-agent orchestration
        let target = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("10.0.0.0/24");
        println!("Campaign mode: targeting {target}\n");
        let result = eugene::agent::run_campaign(model, config, memory, target).await?;
        println!("{result}");
    } else {
        // Single-agent mode (backward compat)
        let task = args
            .get(1)
            .map(|s| s.as_str())
            .unwrap_or("scan the local network with arp-scan");
        println!("Task: {task}\n");
        let agent = create_agent(model, config, memory);
        let result: String = eugene::agent::run_recon_task(&agent, task).await?;
        println!("{result}");
    }

    Ok(())
}
