use super::catalog::{CommandOption, ToolCatalog};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserQuery {
    pub query: String,
    pub n_grams: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScoredCandidate {
    pub option: CommandOption,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScoredTool {
    pub tool: ToolCatalog,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandObject {
    pub base_command: String,
    pub options: Vec<String>,
}
