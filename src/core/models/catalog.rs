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
    pub optimized_data: Vec<OptimizedData>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizedCommandOption {
    pub option_name: String,
    pub description: String,
    pub user_friendly_description: String,
    pub keywords: String,
    pub optimized_data: Vec<OptimizedData>,
}

impl From<&CommandOption> for OptimizedCommandOption {
    fn from(opt: &CommandOption) -> Self {
        Self {
            option_name: opt.option_name.clone(),
            description: opt.description.clone(),
            user_friendly_description: opt.user_friendly_description.clone(),
            keywords: opt.keywords.clone(),
            optimized_data: Vec::new(),
        }
    }
}
