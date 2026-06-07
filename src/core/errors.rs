use std::fmt;

// --- Individual Exception Structs defined in Appendix A ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisambiguationRequiredException {
    pub message: String,
}

impl DisambiguationRequiredException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for DisambiguationRequiredException {}

impl fmt::Display for DisambiguationRequiredException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DisambiguationRequiredException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidConfigurationException {
    pub message: String,
}

impl InvalidConfigurationException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for InvalidConfigurationException {}

impl fmt::Display for InvalidConfigurationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InvalidConfigurationException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectionException {
    pub message: String,
}

impl StorageConnectionException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for StorageConnectionException {}

impl fmt::Display for StorageConnectionException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorageConnectionException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticationException {
    pub message: String,
}

impl AuthenticationException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for AuthenticationException {}

impl fmt::Display for AuthenticationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AuthenticationException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidCatalogException {
    pub message: String,
}

impl InvalidCatalogException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for InvalidCatalogException {}

impl fmt::Display for InvalidCatalogException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InvalidCatalogException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogNotFoundException {
    pub message: String,
}

impl CatalogNotFoundException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for CatalogNotFoundException {}

impl fmt::Display for CatalogNotFoundException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CatalogNotFoundException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCatalogException {
    pub message: String,
}

impl DuplicateCatalogException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for DuplicateCatalogException {}

impl fmt::Display for DuplicateCatalogException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DuplicateCatalogException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintainerNotFoundException {
    pub message: String,
}

impl MaintainerNotFoundException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for MaintainerNotFoundException {}

impl fmt::Display for MaintainerNotFoundException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MaintainerNotFoundException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineExecutionException {
    pub message: String,
}

impl EngineExecutionException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for EngineExecutionException {}

impl fmt::Display for EngineExecutionException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EngineExecutionException: {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationException {
    pub message: String,
}

impl InitializationException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::error::Error for InitializationException {}

impl fmt::Display for InitializationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InitializationException: {}", self.message)
    }
}

// --- Application-wide Error Enum ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Storage(String),
    Ingestion(String),
    Matching(String),
    Validation(String),

    DisambiguationRequired(DisambiguationRequiredException),
    InvalidConfiguration(InvalidConfigurationException),
    StorageConnection(StorageConnectionException),
    Authentication(AuthenticationException),
    InvalidCatalog(InvalidCatalogException),
    CatalogNotFound(CatalogNotFoundException),
    DuplicateCatalog(DuplicateCatalogException),
    MaintainerNotFound(MaintainerNotFoundException),
    EngineExecution(EngineExecutionException),
    Initialization(InitializationException),
}

impl std::error::Error for AppError {}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Storage(msg) => write!(f, "Storage error: {}", msg),
            AppError::Ingestion(msg) => write!(f, "Ingestion error: {}", msg),
            AppError::Matching(msg) => write!(f, "Matching error: {}", msg),
            AppError::Validation(msg) => write!(f, "Validation error: {}", msg),
            AppError::DisambiguationRequired(e) => write!(f, "{}", e),
            AppError::InvalidConfiguration(e) => write!(f, "{}", e),
            AppError::StorageConnection(e) => write!(f, "{}", e),
            AppError::Authentication(e) => write!(f, "{}", e),
            AppError::InvalidCatalog(e) => write!(f, "{}", e),
            AppError::CatalogNotFound(e) => write!(f, "{}", e),
            AppError::DuplicateCatalog(e) => write!(f, "{}", e),
            AppError::MaintainerNotFound(e) => write!(f, "{}", e),
            AppError::EngineExecution(e) => write!(f, "{}", e),
            AppError::Initialization(e) => write!(f, "{}", e),
        }
    }
}

// --- From Implementations for Automatic Conversion via `?` ---

impl From<DisambiguationRequiredException> for AppError {
    fn from(err: DisambiguationRequiredException) -> Self {
        AppError::DisambiguationRequired(err)
    }
}

impl From<InvalidConfigurationException> for AppError {
    fn from(err: InvalidConfigurationException) -> Self {
        AppError::InvalidConfiguration(err)
    }
}

impl From<StorageConnectionException> for AppError {
    fn from(err: StorageConnectionException) -> Self {
        AppError::StorageConnection(err)
    }
}

impl From<AuthenticationException> for AppError {
    fn from(err: AuthenticationException) -> Self {
        AppError::Authentication(err)
    }
}

impl From<InvalidCatalogException> for AppError {
    fn from(err: InvalidCatalogException) -> Self {
        AppError::InvalidCatalog(err)
    }
}

impl From<CatalogNotFoundException> for AppError {
    fn from(err: CatalogNotFoundException) -> Self {
        AppError::CatalogNotFound(err)
    }
}

impl From<DuplicateCatalogException> for AppError {
    fn from(err: DuplicateCatalogException) -> Self {
        AppError::DuplicateCatalog(err)
    }
}

impl From<MaintainerNotFoundException> for AppError {
    fn from(err: MaintainerNotFoundException) -> Self {
        AppError::MaintainerNotFound(err)
    }
}

impl From<EngineExecutionException> for AppError {
    fn from(err: EngineExecutionException) -> Self {
        AppError::EngineExecution(err)
    }
}

impl From<InitializationException> for AppError {
    fn from(err: InitializationException) -> Self {
        AppError::Initialization(err)
    }
}
