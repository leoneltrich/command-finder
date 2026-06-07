use super::catalog::CommandOption;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserQuery {
    pub query: String,
    pub n_grams: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub option: CommandOption,
    pub score: f64,
}
