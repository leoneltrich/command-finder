mod core;
mod ports;
mod adapters;

use std::env;
use crate::adapters::cli_controller::CliController;
use crate::adapters::persistence::PersistenceAdapter;
use crate::core::query_orchestrator::QueryOrchestrator;
use crate::core::models::EndUserConfig;
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;

fn main() {
    let args: Vec<String> = env::args().collect();

    // 1. Instantiate the Storage/Persistence Adapter (outbound StoragePort implementation)
    let storage = PersistenceAdapter::new();

    // 2. Instantiate concrete matching engines
    let keyword_engine = crate::adapters::matching::keyword::KeywordMatchingEngine::new();
    let embedding_engine = crate::adapters::matching::embedding::EmbeddingMatchingEngine::new();

    let matching_engines: Vec<Box<dyn crate::ports::outbound::matching_strategy::MatchingStrategyPort>> = vec![
        Box::new(keyword_engine.clone()),
        Box::new(embedding_engine.clone()),
    ];

    // 3. Instantiate the Core Orchestrator with the storage and matching engines injected
    let query_orchestrator = QueryOrchestrator::new(storage, matching_engines);

    // 4. Instantiate the CLI Controller adapter wrapping the orchestrator
    let controller = CliController::new(query_orchestrator);

    // 5. Routing commands manually (lightweight parsing)
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
        "catalog" => {
            if args.len() < 4 {
                eprintln!("Error: Missing action or payload.");
                eprintln!("Usage: local-assistant catalog <add|update|delete> <payload>");
                std::process::exit(1);
            }
            let action = &args[2];
            let payload = &args[3];
            let auth_key = env::var("AUTH_KEY").unwrap_or_else(|_| "dummy_auth_key".to_string());

            // Instantiate CatalogLifecycleManager with the storage port injected
            let catalog_manager = crate::core::catalog_lifecycle_manager::CatalogLifecycleManager::new(storage);
            let ingestion_api = crate::adapters::ingestion_api::IngestionApi::new(catalog_manager);

            match action.as_str() {
                "add" => {
                    match serde_json::from_str::<crate::core::models::ToolCatalog>(payload) {
                        Ok(catalog) => {
                            match keyword_engine.optimize_catalog(&catalog) {
                                Ok(mut optimized) => {
                                    match embedding_engine.optimize_catalog(&catalog) {
                                        Ok(optimized_emb) => {
                                            optimized.optimized_data.extend(optimized_emb.optimized_data);
                                            for (opt, opt_emb) in optimized.options.iter_mut().zip(optimized_emb.options.iter()) {
                                                opt.optimized_data.extend(opt_emb.optimized_data.clone());
                                            }
                                            match ingestion_api.ingest(&optimized, &auth_key) {
                                                Ok(_) => println!("Catalog successfully added."),
                                                Err(err) => {
                                                    eprintln!("Error adding catalog: {}", err);
                                                    std::process::exit(1);
                                                }
                                            }
                                        }
                                        Err(err) => {
                                            eprintln!("Error optimizing embedding catalog: {}", err);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(err) => {
                                    eprintln!("Error optimizing keyword catalog: {}", err);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("Error parsing JSON catalog: {}", err);
                            std::process::exit(1);
                        }
                    }
                }
                "update" => {
                    match serde_json::from_str::<crate::core::models::ToolCatalog>(payload) {
                        Ok(catalog) => {
                            match keyword_engine.optimize_catalog(&catalog) {
                                Ok(mut optimized) => {
                                    match embedding_engine.optimize_catalog(&catalog) {
                                        Ok(optimized_emb) => {
                                            optimized.optimized_data.extend(optimized_emb.optimized_data);
                                            for (opt, opt_emb) in optimized.options.iter_mut().zip(optimized_emb.options.iter()) {
                                                opt.optimized_data.extend(opt_emb.optimized_data.clone());
                                            }
                                            match ingestion_api.update(&optimized, &auth_key) {
                                                Ok(_) => println!("Catalog successfully updated."),
                                                Err(err) => {
                                                    eprintln!("Error updating catalog: {}", err);
                                                    std::process::exit(1);
                                                }
                                            }
                                        }
                                        Err(err) => {
                                            eprintln!("Error optimizing embedding catalog: {}", err);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(err) => {
                                    eprintln!("Error optimizing keyword catalog: {}", err);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("Error parsing JSON catalog: {}", err);
                            std::process::exit(1);
                        }
                    }
                }
                "delete" => {
                    // For delete, payload is the catalog ID (tool name)
                    match ingestion_api.delete(payload, &auth_key) {
                        Ok(_) => println!("Catalog successfully deleted."),
                        Err(err) => {
                            eprintln!("Error deleting catalog: {}", err);
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    eprintln!("Error: Unknown catalog action '{}'.", action);
                    print_help();
                    std::process::exit(1);
                }
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
    println!("  local-assistant query \"<text>\"                         Resolve natural language query");
    println!("  local-assistant config                                 Manage user configuration interactively");
    println!("  local-assistant catalog <add|update|delete> <payload>  Manage tool catalogs");
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
