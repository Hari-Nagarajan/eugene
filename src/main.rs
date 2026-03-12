use clap::Parser;
use eugene::cli::{Cli, Commands, ScheduleCommands};
use eugene::config::Config;
use eugene::memory::{init_schema, open_memory_store};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let _ = dotenvy::dotenv();
    // Init logger after dotenvy so .env RUST_LOG is available,
    // but systemd Environment= is already in env before main().
    pretty_env_logger::init();
    log::info!("eugene starting, RUST_LOG={}", std::env::var("RUST_LOG").unwrap_or_default());
    let cli = Cli::parse();
    let mut config = Config::load();

    // CLI flags are highest priority (CLI > config.toml > env var > defaults)
    if let Some(ref p) = cli.provider {
        config.provider = Some(p.clone());
    }
    if let Some(ref m) = cli.model {
        config.model = Some(m.clone());
    }

    let config = Arc::new(config);

    match cli.command {
        Commands::Init => {
            eugene::init::run_wizard().await?;
        }
        Commands::Run { target } => {
            require_llm_config(&config)?;
            let db = open_memory_store(&config.db_path).await?;
            init_schema(&db).await?;
            eugene::tui::run_tui(target, config, db).await?;
        }
        Commands::Bot => {
            require_llm_config(&config)?;
            let db = open_memory_store(&config.db_path).await?;
            init_schema(&db).await?;
            eugene::bot::start_bot(config, db).await?;
        }
        Commands::Schedule(subcmd) => {
            require_llm_config(&config)?;
            let db = open_memory_store(&config.db_path).await?;
            init_schema(&db).await?;

            match subcmd {
                ScheduleCommands::Create { cron, prompt } => {
                    let id = eugene::memory::create_schedule(
                        &db,
                        "cli".to_string(),
                        cron,
                        prompt,
                    )
                    .await?;
                    println!("Created schedule: {id}");
                }
                ScheduleCommands::List => {
                    let schedules =
                        eugene::memory::list_schedules(&db, "cli".to_string()).await?;
                    if schedules.is_empty() {
                        println!("No scheduled tasks.");
                    } else {
                        let header = format!(
                            "{:<36} {:<20} {:<10} {}",
                            "ID", "Cron", "Status", "Prompt"
                        );
                        println!("{header}");
                        println!("{}", "-".repeat(80));
                        for s in &schedules {
                            println!(
                                "{:<36} {:<20} {:<10} {}",
                                s.id, s.schedule, s.status, s.prompt
                            );
                        }
                    }
                }
                ScheduleCommands::Delete { id } => {
                    println!("Deleted schedule: {id}");
                    eugene::memory::delete_schedule(&db, id).await?;
                }
                ScheduleCommands::Pause { id } => {
                    println!("Paused schedule: {id}");
                    eugene::memory::pause_schedule(&db, id).await?;
                }
                ScheduleCommands::Resume { id } => {
                    println!("Resumed schedule: {id}");
                    eugene::memory::resume_schedule(&db, id).await?;
                }
            }
        }
        Commands::Wifi { target, no_tui } => {
            require_llm_config(&config)?;
            let db = open_memory_store(&config.db_path).await?;
            init_schema(&db).await?;
            if no_tui {
                let model = eugene::agent::client::create_client(&config)?;
                let (result, run_id) =
                    eugene::agent::run_wifi_campaign(model, config, db.clone(), target.as_deref())
                        .await?;
                println!("{result}");
                let report = eugene::wifi::report::WifiReport::from_run(&db, run_id).await?;
                println!("{}", report.format_cli());
            } else {
                eugene::tui::run_tui_wifi(target, config, db).await?;
            }
        }
        Commands::Service => {
            eugene::service::generate_service()?;
        }
    }

    Ok(())
}

fn require_llm_config(config: &Config) -> Result<(), anyhow::Error> {
    if config.provider.is_none() && config.minimax_api_key.is_none() {
        anyhow::bail!(
            "No LLM provider configured.\n\n\
             Run `eugene init` to set up your provider, or set MINIMAX_API_KEY environment variable.\n\
             You can also create ~/.eugene/config.toml manually."
        );
    }
    Ok(())
}
