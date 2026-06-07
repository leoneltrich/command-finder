use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::{ToolCatalog, CatalogMaintainer, EndUserConfig};

/// Persistence adapter implementing the outbound StoragePort using a normalized SQLite schema with embedding columns.
#[derive(Clone, Copy)]
pub struct PersistenceAdapter;

impl PersistenceAdapter {
    /// Creates a new PersistenceAdapter instance.
    pub fn new() -> Self {
        Self
    }

    /// Establishes connection to the SQLite database and initializes tables if not present.
    fn connect(&self) -> Result<rusqlite::Connection, AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::StorageConnection(
                crate::core::errors::StorageConnectionException::new(format!("Failed to open DB: {}", e))
            ))?;

        // Enable foreign key constraints and WAL journal mode for concurrency resilience (NFR-12)
        let _ = conn.execute("PRAGMA foreign_keys = ON;", []);
        let _ = conn.execute("PRAGMA journal_mode = WAL;", []);

        // Initialize schema with embedding BLOB support
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_catalogs (
                tool_name TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                version TEXT NOT NULL,
                rules TEXT NOT NULL,
                keywords TEXT NOT NULL,
                embedding BLOB
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS command_options (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name TEXT NOT NULL,
                intent TEXT NOT NULL,
                option TEXT NOT NULL,
                keywords TEXT NOT NULL,
                embedding BLOB,
                FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS catalog_maintainers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                auth_key TEXT NOT NULL
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                logging_opt_in INTEGER NOT NULL
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // Initialize default user config if none exists
        conn.execute(
            "INSERT OR IGNORE INTO user_config (id, logging_opt_in) VALUES (1, 0);",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        Ok(conn)
    }
}

impl Default for PersistenceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl StoragePort for PersistenceAdapter {
    // --- Catalog Management ---
    fn save_catalog(&self, catalog: &ToolCatalog) -> Result<bool, AppError> {
        let mut conn = self.connect()?;

        // Use a transaction to ensure atomic parent-child insertions
        let tx = conn.transaction().map_err(|e| AppError::Storage(e.to_string()))?;

        // Enforce uniqueness constraints (DuplicateCatalogException)
        {
            let mut stmt = tx.prepare("SELECT 1 FROM tool_catalogs WHERE tool_name = ?")
                .map_err(|e| AppError::Storage(e.to_string()))?;
            let exists = stmt.exists([&catalog.tool_name])
                .map_err(|e| AppError::Storage(e.to_string()))?;
            if exists {
                return Err(AppError::DuplicateCatalog(
                    crate::core::errors::DuplicateCatalogException::new(format!(
                        "Catalog for tool '{}' already exists.",
                        catalog.tool_name
                    ))
                ));
            }
        }

        let keywords_json = serde_json::to_string(&catalog.keywords)
            .map_err(|e| AppError::Storage(e.to_string()))?;
        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

        // 1. Insert parent
        tx.execute(
            "INSERT INTO tool_catalogs (tool_name, description, version, rules, keywords, embedding) VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                catalog.tool_name,
                catalog.description,
                catalog.version,
                rules_json,
                keywords_json,
                Option::<Vec<u8>>::None
            ],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // 2. Insert children options
        for option in &catalog.options {
            let option_keywords_json = serde_json::to_string(&option.keywords)
                .map_err(|e| AppError::Storage(e.to_string()))?;
            tx.execute(
                "INSERT INTO command_options (tool_name, intent, option, keywords, embedding) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    catalog.tool_name,
                    option.intent,
                    option.option,
                    option_keywords_json,
                    Option::<Vec<u8>>::None
                ],
            ).map_err(|e| AppError::Storage(e.to_string()))?;
        }

        tx.commit().map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }

    fn update_catalog(&self, catalog: &ToolCatalog) -> Result<bool, AppError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| AppError::Storage(e.to_string()))?;

        let keywords_json = serde_json::to_string(&catalog.keywords)
            .map_err(|e| AppError::Storage(e.to_string()))?;
        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

        // 1. Update parent tool catalog info
        let rows_affected = tx.execute(
            "UPDATE tool_catalogs SET description = ?, version = ?, rules = ?, keywords = ? WHERE tool_name = ?",
            rusqlite::params![
                catalog.description,
                catalog.version,
                rules_json,
                keywords_json,
                catalog.tool_name
            ],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(AppError::CatalogNotFound(
                crate::core::errors::CatalogNotFoundException::new(format!(
                    "Catalog for tool '{}' not found for update.",
                    catalog.tool_name
                ))
            ));
        }

        // 2. Remove existing child options
        tx.execute(
            "DELETE FROM command_options WHERE tool_name = ?",
            rusqlite::params![catalog.tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // 3. Re-insert updated child options
        for option in &catalog.options {
            let option_keywords_json = serde_json::to_string(&option.keywords)
                .map_err(|e| AppError::Storage(e.to_string()))?;
            tx.execute(
                "INSERT INTO command_options (tool_name, intent, option, keywords, embedding) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    catalog.tool_name,
                    option.intent,
                    option.option,
                    option_keywords_json,
                    Option::<Vec<u8>>::None
                ],
            ).map_err(|e| AppError::Storage(e.to_string()))?;
        }

        tx.commit().map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }

    fn delete_catalog(&self, tool_name: &str) -> Result<bool, AppError> {
        let conn = self.connect()?;
        let rows_affected = conn.execute(
            "DELETE FROM tool_catalogs WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(AppError::CatalogNotFound(
                crate::core::errors::CatalogNotFoundException::new(format!(
                    "Catalog for tool '{}' not found for deletion.",
                    tool_name
                ))
            ));
        }

        Ok(true)
    }

    fn fetch_catalog(&self, tool_name: &str) -> Result<ToolCatalog, AppError> {
        let conn = self.connect()?;
        
        // 1. Fetch parent tool catalog info
        let mut parent_stmt = conn.prepare(
            "SELECT tool_name, description, version, rules, keywords FROM tool_catalogs WHERE tool_name = ?"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let mut parent_rows = parent_stmt.query(rusqlite::params![tool_name])
            .map_err(|e| AppError::Storage(e.to_string()))?;

        if let Some(row) = parent_rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
            let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
            let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
            let version: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
            let rules_json: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
            let keywords_json: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;

            let keywords: Vec<String> = serde_json::from_str(&keywords_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;
            let rules: crate::core::models::CommandRules = serde_json::from_str(&rules_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;

            // 2. Fetch children options
            let mut options_stmt = conn.prepare(
                "SELECT intent, option, keywords FROM command_options WHERE tool_name = ?"
            ).map_err(|e| AppError::Storage(e.to_string()))?;

            let mapped_options = options_stmt.query_map(rusqlite::params![tool_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }).map_err(|e| AppError::Storage(e.to_string()))?;

            let mut options = Vec::new();
            for option_row in mapped_options {
                let (intent, option_val, opt_keywords_json) = option_row
                    .map_err(|e| AppError::Storage(e.to_string()))?;
                let opt_keywords: Vec<String> = serde_json::from_str(&opt_keywords_json)
                    .map_err(|e| AppError::Storage(e.to_string()))?;

                options.push(crate::core::models::CommandOption {
                    intent,
                    option: option_val,
                    keywords: opt_keywords,
                });
            }

            Ok(ToolCatalog {
                tool_name,
                description,
                keywords,
                version,
                options,
                rules,
            })
        } else {
            Err(AppError::CatalogNotFound(
                crate::core::errors::CatalogNotFoundException::new(format!(
                    "Catalog for tool '{}' not found.",
                    tool_name
                ))
            ))
        }
    }

    fn fetch_all_catalogs(&self) -> Result<Vec<ToolCatalog>, AppError> {
        let conn = self.connect()?;
        let mut parent_stmt = conn.prepare(
            "SELECT tool_name, description, version, rules, keywords FROM tool_catalogs"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let mapped_parents = parent_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e| AppError::Storage(e.to_string()))?;

        let mut catalogs = Vec::new();
        for parent_result in mapped_parents {
            let (tool_name, description, version, rules_json, keywords_json) = parent_result
                .map_err(|e| AppError::Storage(e.to_string()))?;

            let keywords: Vec<String> = serde_json::from_str(&keywords_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;
            let rules: crate::core::models::CommandRules = serde_json::from_str(&rules_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;

            // Fetch children options for this catalog
            let mut options_stmt = conn.prepare(
                "SELECT intent, option, keywords FROM command_options WHERE tool_name = ?"
            ).map_err(|e| AppError::Storage(e.to_string()))?;

            let mapped_options = options_stmt.query_map(rusqlite::params![tool_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }).map_err(|e| AppError::Storage(e.to_string()))?;

            let mut options = Vec::new();
            for option_row in mapped_options {
                let (intent, option_val, opt_keywords_json) = option_row
                    .map_err(|e| AppError::Storage(e.to_string()))?;
                let opt_keywords: Vec<String> = serde_json::from_str(&opt_keywords_json)
                    .map_err(|e| AppError::Storage(e.to_string()))?;

                options.push(crate::core::models::CommandOption {
                    intent,
                    option: option_val,
                    keywords: opt_keywords,
                });
            }

            catalogs.push(ToolCatalog {
                tool_name,
                description,
                keywords,
                version,
                options,
                rules,
            });
        }

        Ok(catalogs)
    }

    // --- Authentication / Maintainer Data ---
    fn save_maintainer(&self, maintainer: &CatalogMaintainer) -> Result<bool, AppError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO catalog_maintainers (id, name, auth_key) VALUES (?, ?, ?)",
            rusqlite::params![maintainer.id, maintainer.name, maintainer.auth_key],
        ).map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }

    fn update_maintainer(&self, maintainer: &CatalogMaintainer) -> Result<bool, AppError> {
        let conn = self.connect()?;
        let rows_affected = conn.execute(
            "UPDATE catalog_maintainers SET name = ?, auth_key = ? WHERE id = ?",
            rusqlite::params![maintainer.name, maintainer.auth_key, maintainer.id],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(AppError::MaintainerNotFound(
                crate::core::errors::MaintainerNotFoundException::new(format!(
                    "Maintainer '{}' not found.",
                    maintainer.id
                ))
            ));
        }

        Ok(true)
    }

    fn fetch_maintainer(&self, maintainer_id: &str) -> Result<CatalogMaintainer, AppError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, auth_key FROM catalog_maintainers WHERE id = ?"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let mut rows = stmt.query(rusqlite::params![maintainer_id])
            .map_err(|e| AppError::Storage(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
            let id: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
            let name: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
            let auth_key: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;

            Ok(CatalogMaintainer { id, name, auth_key })
        } else {
            Err(AppError::MaintainerNotFound(
                crate::core::errors::MaintainerNotFoundException::new(format!(
                    "Maintainer '{}' not found.",
                    maintainer_id
                ))
            ))
        }
    }

    fn delete_maintainer(&self, maintainer_id: &str) -> Result<bool, AppError> {
        let conn = self.connect()?;
        let rows_affected = conn.execute(
            "DELETE FROM catalog_maintainers WHERE id = ?",
            rusqlite::params![maintainer_id],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(AppError::MaintainerNotFound(
                crate::core::errors::MaintainerNotFoundException::new(format!(
                    "Maintainer '{}' not found for deletion.",
                    maintainer_id
                ))
            ));
        }

        Ok(true)
    }

    // --- User Configuration ---
    fn load_configuration(&self) -> Result<EndUserConfig, AppError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT logging_opt_in FROM user_config WHERE id = 1")
            .map_err(|e| AppError::Storage(e.to_string()))?;
        let logging_opt_in: i32 = stmt.query_row([], |row| row.get(0))
            .map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(EndUserConfig {
            logging_opt_in: logging_opt_in != 0,
        })
    }

    fn save_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        let conn = self.connect()?;
        let val = if config.logging_opt_in { 1 } else { 0 };
        conn.execute(
            "INSERT OR REPLACE INTO user_config (id, logging_opt_in) VALUES (1, ?)",
            rusqlite::params![val],
        ).map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }
}
