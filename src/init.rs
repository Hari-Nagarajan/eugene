use dialoguer::{Input, Password, Select, theme::ColorfulTheme};
use rig::client::verify::{VerifyClient, VerifyError};
use rig::providers::{anthropic, openrouter};

use crate::config::{EugeneConfig, LlmConfig};

/// Run the interactive configuration wizard.
///
/// Walks the user through provider selection, API key entry, model choice,
/// validates the API key, and saves the config to ~/.eugene/config.toml.
///
/// This function is async because it calls rig's VerifyClient::verify()
/// for API key validation. The dialoguer prompts block the current thread,
/// which is fine since the tokio runtime is idle during the wizard.
pub async fn run_wizard() -> Result<(), anyhow::Error> {
    let theme = ColorfulTheme::default();

    println!("\nEugene Configuration Wizard\n");

    // Step 1: Provider selection
    let providers = vec![
        "MiniMax (MiniMax-M2.5)",
        "OpenRouter (Claude, GPT, Gemini, etc.)",
    ];
    let selection = Select::with_theme(&theme)
        .with_prompt("Select your LLM provider")
        .items(&providers)
        .default(0)
        .interact()?;
    let provider = match selection {
        0 => "minimax",
        1 => "openrouter",
        _ => unreachable!(),
    };

    // Step 2: API key
    let api_key = Password::with_theme(&theme)
        .with_prompt("Enter your API key")
        .interact()?;

    // Step 3: Model selection (provider-specific)
    let model = select_model(&theme, provider)?;

    // Step 4: Validate API key
    println!("Validating API key...");
    validate_api_key(provider, &api_key).await?;

    // Step 5: Save config (preserve non-LLM sections)
    let mut existing = EugeneConfig::load_from_file();
    existing.llm = LlmConfig {
        provider: Some(provider.to_string()),
        api_key: Some(api_key),
        model: Some(model),
        base_url: None,
        llm_log_level: None,
    };
    existing.save_to_file()?;

    println!("\nConfiguration saved to ~/.eugene/config.toml");
    println!("You can now run `eugene run` to start a campaign.");

    Ok(())
}

fn select_model(theme: &ColorfulTheme, provider: &str) -> Result<String, anyhow::Error> {
    match provider {
        "minimax" => {
            let models = vec!["MiniMax-M2.5"];
            let idx = Select::with_theme(theme)
                .with_prompt("Select model")
                .items(&models)
                .default(0)
                .interact()?;
            Ok(models[idx].to_string())
        }
        "openrouter" => {
            let models = vec![
                "anthropic/claude-sonnet-4",
                "openai/gpt-4o",
                "google/gemini-2.0-flash-001",
                "Other (enter manually)",
            ];
            let idx = Select::with_theme(theme)
                .with_prompt("Select model")
                .items(&models)
                .default(0)
                .interact()?;
            if idx == models.len() - 1 {
                let custom: String = Input::with_theme(theme)
                    .with_prompt("Enter model name (e.g., anthropic/claude-sonnet-4)")
                    .interact_text()?;
                Ok(custom)
            } else {
                Ok(models[idx].to_string())
            }
        }
        _ => unreachable!(),
    }
}

async fn validate_api_key(provider: &str, api_key: &str) -> Result<(), anyhow::Error> {
    match provider {
        "openrouter" => {
            let client: openrouter::Client = openrouter::Client::builder()
                .api_key(api_key)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build OpenRouter client: {e}"))?;
            client
                .verify()
                .await
                .map_err(|e| anyhow::anyhow!("Invalid API key for OpenRouter: {e}"))
        }
        "minimax" => {
            let client: anthropic::Client = anthropic::Client::builder()
                .api_key(api_key)
                .base_url("https://api.minimax.io/anthropic")
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build MiniMax client: {e}"))?;
            match client.verify().await {
                Ok(()) => Ok(()),
                Err(VerifyError::InvalidAuthentication) => {
                    Err(anyhow::anyhow!("Invalid API key for MiniMax"))
                }
                Err(_) => {
                    // MiniMax may not support the /v1/models verify endpoint
                    eprintln!("Warning: Could not verify MiniMax API key (endpoint not available). Key will be validated on first use.");
                    Ok(())
                }
            }
        }
        _ => unreachable!(),
    }
}
