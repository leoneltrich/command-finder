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
