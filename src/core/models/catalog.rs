#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCatalog {
    pub tool_name: String,
    pub description: String,
    pub user_friendly_description: String,
    pub keywords: String,
    pub version: String,
    pub options: Vec<CommandOption>,
    pub rules: CommandRules,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOption {
    pub option_name: String,
    pub description: String,
    pub user_friendly_description: String,
    pub keywords: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandRules(pub serde_json::Value);


