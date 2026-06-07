#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserQuery {
    pub query: String,
    pub n_grams: Option<Vec<String>>,
}
