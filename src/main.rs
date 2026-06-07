mod core;
mod ports;
mod adapters;

use std::env;
use crate::adapters::cli_controller::CliController;
use crate::core::query_orchestrator::QueryOrchestrator;
use crate::core::models::EndUserConfig;
use crate::core::errors::AppError;

fn main() {
    let args: Vec<String> = env::args().collect();

    // 1. Instantiate the Core Orchestrator (implements UserCommandPort)
    let query_orchestrator = QueryOrchestrator::new();

    // 2. Instantiate the CLI Controller adapter
    let controller = CliController::new(query_orchestrator);

    // 3. Routing commands manually (lightweight parsing)
    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "query" => {
            if args.len() < 3 {
                eprintln!("Error: Missing query string.");
                eprintln!("Usage: local-assistant query \"<text>\"");
                std::process::exit(1);
            }
            let user_query = &args[2];
            match controller.handle_query(user_query) {
                Ok(resolved_cmd) => {
                    println!("{}", resolved_cmd);
                }
                Err(err) => {
                    eprintln!("Error resolving query: {}", err);
                    std::process::exit(1);
                }
            }
        }
        "config" => {
            if let Err(err) = handle_config_flow(&controller) {
                eprintln!("Error managing configuration: {}", err);
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("Error: Unknown command '{}'.", args[1]);
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("local-assistant - Natural Language Command Option Retrieval");
    println!("\nUsage:");
    println!("  local-assistant query \"<text>\"  Resolve natural language query");
    println!("  local-assistant config          Manage user configuration interactively");
}

fn handle_config_flow<P>(controller: &CliController<P>) -> Result<(), AppError>
where
    P: crate::ports::inbound::user_command::UserCommandPort,
{
    // Read the current configuration
    let current_config = controller.read_config()?;
    println!("Current Configuration: logging_opt_in={}", current_config.logging_opt_in);

    // Prompt user using inquire
    let opt_in = inquire::Confirm::new("Enable diagnostic logging opt-in?")
        .with_default(current_config.logging_opt_in)
        .prompt()
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let new_config = EndUserConfig {
        logging_opt_in: opt_in,
    };

    controller.update_config(&new_config)?;
    println!("Configuration updated successfully.");
    Ok(())
}
