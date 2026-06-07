#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandCatalog {
    pub tool_name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub version: String,
    pub options: Vec<CommandOption>,
    pub rules: CommandRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOption {
    pub intent: String,
    pub keywords: Vec<String>,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRules {
    pub rules: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMaintainer {
    pub id: String,
    pub name: String,
    pub auth_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndUserConfig {
    pub confidence_threshold: String,
    pub logging_opt_in: bool,
    pub matching_strategy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub query: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub option: CommandOption,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandItem {
    pub base: String,
    pub intent: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserQuery {
    pub query: String,
    pub n_grams: Option<Vec<String>>,
}