#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolCatalog {
    pub tool_name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub version: String,
    pub options: Vec<CommandOption>,
    pub rules: CommandRules,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandOption {
    pub intent: String,
    pub keywords: Vec<String>,
    pub option: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandRules {
    pub rules: String,
}
