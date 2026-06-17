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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizedData {
    pub key: String,
    pub data: Vec<u8>,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizedToolCatalog {
    pub tool_name: String,
    pub description: String,
    pub user_friendly_description: String,
    pub keywords: String,
    pub version: String,
    pub options: Vec<OptimizedCommandOption>,
    pub rules: CommandRules,
    pub optimized_data: Option<OptimizedData>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizedCommandOption {
    pub option_name: String,
    pub description: String,
    pub user_friendly_description: String,
    pub keywords: String,
    pub optimized_data: Option<OptimizedData>,
}
