use crate::ports::inbound::user_command::UserCommandPort;
use crate::ports::outbound::storage::StoragePort;
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{EndUserConfig, ScoredTool, ScoredCandidate};
use crate::core::syntactical_validator::SyntacticalValidator;
use crate::core::similarity_rank_aggregator::SimilarityRankAggregator;

/// Core interactor responsible for query resolution and user configuration.
pub struct QueryOrchestrator<S: StoragePort> {
    storage_port: S,
    matching_engines: Vec<Box<dyn MatchingStrategyPort>>,
    validator: SyntacticalValidator,
    rank_aggregator: SimilarityRankAggregator,
}

impl<S: StoragePort> QueryOrchestrator<S> {
    /// Creates a new instance of the QueryOrchestrator.
    pub fn new(storage_port: S, matching_engines: Vec<Box<dyn MatchingStrategyPort>>) -> Self {
        Self {
            storage_port,
            matching_engines,
            validator: SyntacticalValidator::new(),
            rank_aggregator: SimilarityRankAggregator::new(),
        }
    }

    fn format_disambiguation_message(
        &self,
        tool: &crate::core::models::ToolCatalog,
        options: &[ScoredCandidate],
    ) -> String {
        let mut msg = String::new();
        msg.push_str("\u{001b}[1;31mThe query could not be resolved. Please revise your query.\n\nThe following tool and options were matched, some of the options are conflicting with each other:\u{001b}[0m\n\n");

        let mut max_name_len = tool.tool_name.len();
        for opt in options {
            max_name_len = max_name_len.max(opt.option.option_name.len());
        }

        let terminal_width: usize = 80;
        let indent: usize = max_name_len + 3;

        // Tool line
        let tool_prefix = format!("{:width$} - ", tool.tool_name, width = max_name_len);
        let wrapped_tool_desc = wrap_text(&tool.user_friendly_description, terminal_width.saturating_sub(indent).max(20));
        msg.push_str(&tool_prefix);
        msg.push_str(&wrapped_tool_desc[0]);
        msg.push('\n');
        for line in &wrapped_tool_desc[1..] {
            msg.push_str(&" ".repeat(indent));
            msg.push_str(line);
            msg.push('\n');
        }

        // Options
        for opt in options {
            let opt_prefix = format!("{:width$} - ", opt.option.option_name, width = max_name_len);
            let wrapped_opt_desc = wrap_text(&opt.option.user_friendly_description, terminal_width.saturating_sub(indent).max(20));
            msg.push_str(&opt_prefix);
            msg.push_str(&wrapped_opt_desc[0]);
            msg.push('\n');
            for line in &wrapped_opt_desc[1..] {
                msg.push_str(&" ".repeat(indent));
                msg.push_str(line);
                msg.push('\n');
            }
        }

        msg.trim_end_matches('\n').to_string()
    }

    fn format_success_message(&self, command: &str) -> String {
        let mut msg = String::new();
        msg.push_str(&format!("\u{001b}[1;32m{}\u{001b}[0m\n\n", command));
        
        msg.push_str("Note: This command may contain placeholders. Please substitute them with actual values before running the command.\n\n");
        
        msg.push_str("Placeholder Reference:\n");
        msg.push_str("• [SOME_DESCRIPTIVE_TEXT]\n");
        msg.push_str("  Description: Represents a variable parameter. Substitute the bracketed block with the actual value described (e.g. a file name, directory, or custom string).\n");
        msg.push_str("  Example:     tool [file_name] -> tool data.txt\n\n");
        
        msg.push_str("• [option1|option2]\n");
        msg.push_str("  Description: Represents a choice between multiple options. You must choose exactly one option that fits your specific needs.\n");
        msg.push_str("  Example:     tool --backup=[all|older] -> tool --backup=older\n\n");
        
        msg.push_str("• [\"descriptive_text\"]\n");
        msg.push_str("  Description: Represents a variable parameter where the surrounding double quotes (\"\") must be preserved around your input value.\n");
        msg.push_str("  Example:     tool --message=[\"commit message\"] -> tool --message=\"Initial commit\"\n\n");
        
        msg.push_str("• /source/path\n");
        msg.push_str("  Description: Represents the full absolute path of the source file or directory. To target a file in your current working directory, run \"pwd\" in your terminal and append the file name to the printed path.\n");
        msg.push_str("  Example:     /source/path --> pwd outputs: /tmp -> /tmp/data.txt\n\n");
        
        msg.push_str("• /destination/path\n");
        msg.push_str("  Description: Represents the full absolute path of the destination file or directory.\n");
        msg.push_str("  Example:     /destination/path -> pwd outputs: /tmp -> /tmp/destination");
        
        msg
    }
}

impl<S: StoragePort> UserCommandPort for QueryOrchestrator<S> {
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError> {
        // 1. Construct UserQuery object
        let user_query = crate::core::models::UserQuery {
            query: raw_query.to_string(),
            n_grams: None,
        };

        // 2. Find matching tools (catalogs) from all engines
        let mut engine_tool_results = Vec::new();
        for engine in &self.matching_engines {
            engine.load_engines()?;
            let tools = engine.find_tools(&user_query)?;
            engine_tool_results.push((tools, engine.tool_weight()));
        }

        // Prepare parameters for the aggregator
        let aggregator_inputs: Vec<(&[ScoredTool], f64)> = engine_tool_results
            .iter()
            .map(|(tools, weight)| (tools.as_slice(), *weight))
            .collect();

        // 3. Aggregate tool matches using SimilarityRankAggregator
        let aggregated_tools = self.rank_aggregator.aggregate_tools(&aggregator_inputs)?;

        let mut tokens = Vec::new();

        // 4. Retrieve options for the highest scored tool (only the best ranked tool is used)
        if let Some(top_tool) = aggregated_tools.first() {
            let tool_name = &top_tool.tool.tool_name;
            tokens.push(tool_name.clone());
            
            let mut engine_option_results = Vec::new();
            for engine in &self.matching_engines {
                if let Ok(options) = engine.find_options(&user_query, tool_name) {
                    engine_option_results.push((options, engine.option_weight()));
                }
            }

            // Prepare inputs for the aggregator
            let option_aggregator_inputs: Vec<(&[ScoredCandidate], f64)> = engine_option_results
                .iter()
                .map(|(options, weight)| (options.as_slice(), *weight))
                .collect();

            // Post-aggregation Otsu config
            let post_agg_otsu_config = crate::adapters::matching::otsu::OtsuCutoffConfig::new(
                0.50, // Alpha
                0.00, // Hard floor
                3.00, // Multiplier
            );

            // Aggregate options using SimilarityRankAggregator
            let aggregated_options = self.rank_aggregator.aggregate_options(
                &option_aggregator_inputs,
                &post_agg_otsu_config,
            )?;

            // Add all aggregated options to our token array
            for opt in &aggregated_options {
                tokens.push(opt.option.option_name.clone());
            }

            // Validate the tokens against the tool's syntactical rules
            match self.validator.validate(&tokens, &top_tool.tool.rules) {
                Ok(validated_command) => {
                    let formatted_success = self.format_success_message(&validated_command);
                    Ok(formatted_success)
                }
                Err(_) => {
                    let formatted_msg = self.format_disambiguation_message(&top_tool.tool, &aggregated_options);
                    Ok(formatted_msg)
                }
            }
        } else {
            Ok("Query unclear".to_string())
        }
    }

    fn update_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        self.storage_port.save_configuration(config)
    }

    fn read_configuration(&self) -> Result<EndUserConfig, AppError> {
        self.storage_port.load_configuration()
    }
}

fn wrap_text(text: &str, line_width: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if current_line.len() + 1 + word.len() <= line_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ToolCatalog, CommandRules, CommandOption, ScoredCandidate};

    #[test]
    fn test_wrap_text() {
        let text = "one two three four five six";
        let lines = wrap_text(text, 10);
        assert_eq!(lines, vec!["one two", "three four", "five six"]);

        let long_word = "supercalifragilisticexpialidocious";
        let lines_long = wrap_text(long_word, 10);
        assert_eq!(lines_long, vec![long_word]);
    }

    #[test]
    fn test_format_disambiguation_message() {
        let tool = ToolCatalog {
            tool_name: "test-tool".to_string(),
            description: "Some description".to_string(),
            user_friendly_description: "This is a very long user friendly description of the test tool that should wrap to multiple lines correctly.".to_string(),
            keywords: "test".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
        };

        let options = vec![
            ScoredCandidate {
                option: CommandOption {
                    option_name: "-s".to_string(),
                    description: "short".to_string(),
                    user_friendly_description: "A short flag description.".to_string(),
                    keywords: "short".to_string(),
                },
                score: 1.0,
            },
            ScoredCandidate {
                option: CommandOption {
                    option_name: "--very-long-option-name".to_string(),
                    description: "long description".to_string(),
                    user_friendly_description: "This option does something complex and has a long user friendly description that should also wrap.".to_string(),
                    keywords: "long".to_string(),
                },
                score: 0.9,
            },
        ];

        struct MockStorage;
        impl crate::ports::outbound::storage::StoragePort for MockStorage {
            fn save_catalog(&self, _: &ToolCatalog) -> Result<bool, AppError> { Ok(true) }
            fn update_catalog(&self, _: &ToolCatalog) -> Result<bool, AppError> { Ok(true) }
            fn delete_catalog(&self, _: &str) -> Result<bool, AppError> { Ok(true) }
            fn fetch_catalog(&self, _: &str) -> Result<ToolCatalog, AppError> {
                unimplemented!()
            }
            fn fetch_all_catalogs(&self) -> Result<Vec<ToolCatalog>, AppError> { Ok(vec![]) }
            fn save_maintainer(&self, _: &crate::core::models::CatalogMaintainer) -> Result<bool, AppError> { Ok(true) }
            fn update_maintainer(&self, _: &crate::core::models::CatalogMaintainer) -> Result<bool, AppError> { Ok(true) }
            fn fetch_maintainer(&self, _: &str) -> Result<crate::core::models::CatalogMaintainer, AppError> {
                unimplemented!()
            }
            fn delete_maintainer(&self, _: &str) -> Result<bool, AppError> { Ok(true) }
            fn load_configuration(&self) -> Result<EndUserConfig, AppError> {
                Ok(EndUserConfig { logging_opt_in: false })
            }
            fn save_configuration(&self, _: &EndUserConfig) -> Result<bool, AppError> { Ok(true) }
        }

        let orchestrator = QueryOrchestrator::new(MockStorage, vec![]);
        let msg = orchestrator.format_disambiguation_message(&tool, &options);

        assert!(msg.starts_with("\u{001b}[1;31mThe query could not be resolved. Please revise your query."));

        let lines: Vec<&str> = msg.lines().collect();
        let tool_line_idx = lines.iter().position(|l| l.starts_with("test-tool ")).unwrap();
        assert!(lines[tool_line_idx].starts_with("test-tool               - "));

        let tool_next_line = lines[tool_line_idx + 1];
        let tool_spaces = tool_next_line.chars().take_while(|&c| c == ' ').count();
        assert_eq!(tool_spaces, 26, "tool next line: {:?}", tool_next_line);
        assert!(!tool_next_line.trim().is_empty());

        let opt_line_idx = lines.iter().position(|l| l.starts_with("--very-long-option-name ")).unwrap();
        assert!(lines[opt_line_idx].starts_with("--very-long-option-name - "));

        let opt_next_line = lines[opt_line_idx + 1];
        let opt_spaces = opt_next_line.chars().take_while(|&c| c == ' ').count();
        assert_eq!(opt_spaces, 26, "opt next line: {:?}", opt_next_line);
        assert!(!opt_next_line.trim().is_empty());
    }

    #[test]
    fn test_format_success_message() {
        struct MockStorage;
        impl crate::ports::outbound::storage::StoragePort for MockStorage {
            fn save_catalog(&self, _: &ToolCatalog) -> Result<bool, AppError> { Ok(true) }
            fn update_catalog(&self, _: &ToolCatalog) -> Result<bool, AppError> { Ok(true) }
            fn delete_catalog(&self, _: &str) -> Result<bool, AppError> { Ok(true) }
            fn fetch_catalog(&self, _: &str) -> Result<ToolCatalog, AppError> { unimplemented!() }
            fn fetch_all_catalogs(&self) -> Result<Vec<ToolCatalog>, AppError> { Ok(vec![]) }
            fn save_maintainer(&self, _: &crate::core::models::CatalogMaintainer) -> Result<bool, AppError> { Ok(true) }
            fn update_maintainer(&self, _: &crate::core::models::CatalogMaintainer) -> Result<bool, AppError> { Ok(true) }
            fn fetch_maintainer(&self, _: &str) -> Result<crate::core::models::CatalogMaintainer, AppError> { unimplemented!() }
            fn delete_maintainer(&self, _: &str) -> Result<bool, AppError> { Ok(true) }
            fn load_configuration(&self) -> Result<EndUserConfig, AppError> { Ok(EndUserConfig { logging_opt_in: false }) }
            fn save_configuration(&self, _: &EndUserConfig) -> Result<bool, AppError> { Ok(true) }
        }

        let orchestrator = QueryOrchestrator::new(MockStorage, vec![]);
        let msg = orchestrator.format_success_message("cp -R source dest");
        
        assert!(msg.starts_with("\u{001b}[1;32mcp -R source dest\u{001b}[0m\n\n"));
        assert!(msg.contains("Note: This command may contain placeholders."));
        assert!(msg.contains("[SOME_DESCRIPTIVE_TEXT]"));
        assert!(msg.contains("[option1|option2]"));
        assert!(msg.contains("[\"descriptive_text\"]"));
        assert!(msg.contains("/source/path"));
        assert!(msg.contains("/destination/path"));
    }
}
