use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::{ToolCatalog, CatalogMaintainer, EndUserConfig, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData};

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

        // Detect if legacy database schema migration is needed (e.g. if 'intent' or hardcoded 'embedding' column exists)
        let table_info_migration_needed = if let Ok(mut stmt) = conn.prepare("PRAGMA table_info(command_options);") {
            let mut rows = stmt.query([]).unwrap();
            let mut migration_needed = false;
            while let Some(row) = rows.next().unwrap() {
                let name: String = row.get(1).unwrap();
                if name == "intent" || name == "embedding" {
                    migration_needed = true;
                }
            }
            migration_needed
        } else {
            false
        };

        if table_info_migration_needed {
            // Drop old tables to recreate with new schema
            let _ = conn.execute("DROP TABLE IF EXISTS command_options;", []);
            let _ = conn.execute("DROP TABLE IF EXISTS tool_catalogs;", []);
        }

        // Initialize schema without the redundant embedding BLOB column
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_catalogs (
                tool_name TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                user_friendly_description TEXT NOT NULL,
                version TEXT NOT NULL,
                rules TEXT NOT NULL,
                keywords TEXT NOT NULL
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS command_options (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name TEXT NOT NULL,
                option_name TEXT NOT NULL,
                description TEXT NOT NULL,
                user_friendly_description TEXT NOT NULL,
                keywords TEXT NOT NULL,
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

// Helper functions for dynamic custom column schema updates
fn column_exists(conn: &rusqlite::Connection, table_name: &str, column_name: &str) -> bool {
    if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({});", table_name)) {
        if let Ok(mut rows) = stmt.query([]) {
            while let Some(row) = rows.next().unwrap_or(None) {
                if let Ok(name) = row.get::<_, String>(1) {
                    if name == column_name {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn add_column_if_not_exists(conn: &rusqlite::Connection, table_name: &str, column_name: &str, data_type: &str) -> Result<(), AppError> {
    if !column_exists(conn, table_name, column_name) {
        // Enforce safe column name formatting (alphanumeric and underscores only to prevent SQL injection)
        if column_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let sql = format!("ALTER TABLE {} ADD COLUMN {} {};", table_name, column_name, data_type);
            conn.execute(&sql, []).map_err(|e| AppError::Storage(e.to_string()))?;
        } else {
            return Err(AppError::Storage(format!("Invalid custom column name: {}", column_name)));
        }
    }
    Ok(())
}

const STANDARD_TOOL_COLS: &[&str] = &[
    "tool_name",
    "description",
    "user_friendly_description",
    "version",
    "rules",
    "keywords",
];

const STANDARD_OPTION_COLS: &[&str] = &[
    "id",
    "tool_name",
    "option_name",
    "description",
    "user_friendly_description",
    "keywords",
];

fn extract_optimized_data(row: &rusqlite::Row, col_names: &[&str], standard_cols: &[&str]) -> Option<OptimizedData> {
    for (i, name) in col_names.iter().enumerate() {
        if !standard_cols.contains(name) {
            if let Ok(Some(data)) = row.get::<_, Option<Vec<u8>>>(i) {
                return Some(OptimizedData {
                    key: name.to_string(),
                    data,
                    data_type: "BLOB".to_string(),
                });
            }
        }
    }
    None
}

impl StoragePort for PersistenceAdapter {
    // --- Catalog Management ---
    fn save_catalog(&self, catalog: &OptimizedToolCatalog) -> Result<bool, AppError> {
        let mut conn = self.connect()?;

        // Use a transaction to ensure atomic parent-child insertions and dynamic schema changes
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

        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

        // Ensure custom dynamic column for tool is added if present
        if let Some(ref opt_data) = catalog.optimized_data {
            add_column_if_not_exists(&tx, "tool_catalogs", &opt_data.key, &opt_data.data_type)?;
        }

        // Ensure custom dynamic columns for options are added if present
        for option in &catalog.options {
            if let Some(ref opt_data) = option.optimized_data {
                add_column_if_not_exists(&tx, "command_options", &opt_data.key, &opt_data.data_type)?;
            }
        }

        // 1. Insert parent tool catalog
        let (insert_sql, params) = if let Some(ref opt_data) = catalog.optimized_data {
            let sql = format!(
                "INSERT INTO tool_catalogs (tool_name, description, user_friendly_description, version, rules, keywords, {}) VALUES (?, ?, ?, ?, ?, ?, ?)",
                opt_data.key
            );
            (sql, rusqlite::params![
                catalog.tool_name,
                catalog.description,
                catalog.user_friendly_description,
                catalog.version,
                rules_json,
                catalog.keywords,
                opt_data.data
            ])
        } else {
            let sql = "INSERT INTO tool_catalogs (tool_name, description, user_friendly_description, version, rules, keywords) VALUES (?, ?, ?, ?, ?, ?)".to_string();
            (sql, rusqlite::params![
                catalog.tool_name,
                catalog.description,
                catalog.user_friendly_description,
                catalog.version,
                rules_json,
                catalog.keywords
            ])
        };

        tx.execute(&insert_sql, params).map_err(|e| AppError::Storage(e.to_string()))?;

        // 2. Insert children options
        for option in &catalog.options {
            let (option_sql, option_params) = if let Some(ref opt_data) = option.optimized_data {
                let sql = format!(
                    "INSERT INTO command_options (tool_name, option_name, description, user_friendly_description, keywords, {}) VALUES (?, ?, ?, ?, ?, ?)",
                    opt_data.key
                );
                (sql, rusqlite::params![
                    catalog.tool_name,
                    option.option_name,
                    option.description,
                    option.user_friendly_description,
                    option.keywords,
                    opt_data.data
                ])
            } else {
                let sql = "INSERT INTO command_options (tool_name, option_name, description, user_friendly_description, keywords) VALUES (?, ?, ?, ?, ?)".to_string();
                (sql, rusqlite::params![
                    catalog.tool_name,
                    option.option_name,
                    option.description,
                    option.user_friendly_description,
                    option.keywords
                ])
            };

            tx.execute(&option_sql, option_params).map_err(|e| AppError::Storage(e.to_string()))?;
        }

        tx.commit().map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }

    fn update_catalog(&self, catalog: &OptimizedToolCatalog) -> Result<bool, AppError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| AppError::Storage(e.to_string()))?;

        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

        // Ensure custom dynamic column for tool is added if present
        if let Some(ref opt_data) = catalog.optimized_data {
            add_column_if_not_exists(&tx, "tool_catalogs", &opt_data.key, &opt_data.data_type)?;
        }

        // Ensure custom dynamic columns for options are added if present
        for option in &catalog.options {
            if let Some(ref opt_data) = option.optimized_data {
                add_column_if_not_exists(&tx, "command_options", &opt_data.key, &opt_data.data_type)?;
            }
        }

        // 1. Update parent tool catalog info
        let (update_sql, params) = if let Some(ref opt_data) = catalog.optimized_data {
            let sql = format!(
                "UPDATE tool_catalogs SET description = ?, user_friendly_description = ?, version = ?, rules = ?, keywords = ?, {} = ? WHERE tool_name = ?",
                opt_data.key
            );
            (sql, rusqlite::params![
                catalog.description,
                catalog.user_friendly_description,
                catalog.version,
                rules_json,
                catalog.keywords,
                opt_data.data,
                catalog.tool_name
            ])
        } else {
            let sql = "UPDATE tool_catalogs SET description = ?, user_friendly_description = ?, version = ?, rules = ?, keywords = ? WHERE tool_name = ?".to_string();
            (sql, rusqlite::params![
                catalog.description,
                catalog.user_friendly_description,
                catalog.version,
                rules_json,
                catalog.keywords,
                catalog.tool_name
            ])
        };

        let rows_affected = tx.execute(&update_sql, params).map_err(|e| AppError::Storage(e.to_string()))?;

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
            let (option_sql, option_params) = if let Some(ref opt_data) = option.optimized_data {
                let sql = format!(
                    "INSERT INTO command_options (tool_name, option_name, description, user_friendly_description, keywords, {}) VALUES (?, ?, ?, ?, ?, ?)",
                    opt_data.key
                );
                (sql, rusqlite::params![
                    catalog.tool_name,
                    option.option_name,
                    option.description,
                    option.user_friendly_description,
                    option.keywords,
                    opt_data.data
                ])
            } else {
                let sql = "INSERT INTO command_options (tool_name, option_name, description, user_friendly_description, keywords) VALUES (?, ?, ?, ?, ?)".to_string();
                (sql, rusqlite::params![
                    catalog.tool_name,
                    option.option_name,
                    option.description,
                    option.user_friendly_description,
                    option.keywords
                ])
            };

            tx.execute(&option_sql, option_params).map_err(|e| AppError::Storage(e.to_string()))?;
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

    fn fetch_catalog(&self, tool_name: &str) -> Result<OptimizedToolCatalog, AppError> {
        let conn = self.connect()?;
        
        // 1. Fetch parent tool catalog info
        let mut parent_stmt = conn.prepare(
            "SELECT * FROM tool_catalogs WHERE tool_name = ?"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let col_names: Vec<String> = parent_stmt.column_names().iter().map(|s| s.to_string()).collect();
        let col_names_refs: Vec<&str> = col_names.iter().map(|s| s.as_str()).collect();

        let mut parent_rows = parent_stmt.query(rusqlite::params![tool_name])
            .map_err(|e| AppError::Storage(e.to_string()))?;

        if let Some(row) = parent_rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
            let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
            let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
            let user_friendly_description: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
            let version: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
            let rules_json: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
            let keywords: String = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;

            let rules: crate::core::models::CommandRules = serde_json::from_str(&rules_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;

            // Extract dynamic custom optimized data
            let optimized_data = extract_optimized_data(row, &col_names_refs, STANDARD_TOOL_COLS);

            // 2. Fetch children options
            let mut options_stmt = conn.prepare(
                "SELECT * FROM command_options WHERE tool_name = ?"
            ).map_err(|e| AppError::Storage(e.to_string()))?;

            let opt_col_names: Vec<String> = options_stmt.column_names().iter().map(|s| s.to_string()).collect();
            let opt_col_names_refs: Vec<&str> = opt_col_names.iter().map(|s| s.as_str()).collect();

            // Prepare dynamic column inspection mapping
            let mut rows = options_stmt.query(rusqlite::params![tool_name])
                .map_err(|e| AppError::Storage(e.to_string()))?;

            let mut options = Vec::new();
            while let Some(option_row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
                let option_name: String = option_row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
                let description: String = option_row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
                let user_friendly_description: String = option_row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
                let keywords: String = option_row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;

                let opt_optimized_data = extract_optimized_data(option_row, &opt_col_names_refs, STANDARD_OPTION_COLS);

                options.push(OptimizedCommandOption {
                    option_name,
                    description,
                    user_friendly_description,
                    keywords,
                    optimized_data: opt_optimized_data,
                });
            }

            Ok(OptimizedToolCatalog {
                tool_name,
                description,
                user_friendly_description,
                keywords,
                version,
                options,
                rules,
                optimized_data,
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

    fn fetch_all_catalogs(&self) -> Result<Vec<OptimizedToolCatalog>, AppError> {
        let conn = self.connect()?;
        let mut parent_stmt = conn.prepare(
            "SELECT * FROM tool_catalogs"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let col_names: Vec<String> = parent_stmt.column_names().iter().map(|s| s.to_string()).collect();
        let col_names_refs: Vec<&str> = col_names.iter().map(|s| s.as_str()).collect();

        let mut parent_rows = parent_stmt.query([]).map_err(|e| AppError::Storage(e.to_string()))?;

        let mut catalogs = Vec::new();
        while let Some(row) = parent_rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
            let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
            let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
            let user_friendly_description: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
            let version: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
            let rules_json: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
            let keywords: String = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;

            let rules: crate::core::models::CommandRules = serde_json::from_str(&rules_json)
                .map_err(|e| AppError::Storage(e.to_string()))?;

            let optimized_data = extract_optimized_data(row, &col_names_refs, STANDARD_TOOL_COLS);

            // Fetch children options for this catalog
            let mut options_stmt = conn.prepare(
                "SELECT * FROM command_options WHERE tool_name = ?"
            ).map_err(|e| AppError::Storage(e.to_string()))?;

            let opt_col_names: Vec<String> = options_stmt.column_names().iter().map(|s| s.to_string()).collect();
            let opt_col_names_refs: Vec<&str> = opt_col_names.iter().map(|s| s.as_str()).collect();

            let mut rows = options_stmt.query(rusqlite::params![tool_name])
                .map_err(|e| AppError::Storage(e.to_string()))?;

            let mut options = Vec::new();
            while let Some(option_row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
                let option_name: String = option_row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
                let description: String = option_row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
                let user_friendly_description: String = option_row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
                let keywords: String = option_row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;

                let opt_optimized_data = extract_optimized_data(option_row, &opt_col_names_refs, STANDARD_OPTION_COLS);

                options.push(OptimizedCommandOption {
                    option_name,
                    description,
                    user_friendly_description,
                    keywords,
                    optimized_data: opt_optimized_data,
                });
            }

            catalogs.push(OptimizedToolCatalog {
                tool_name,
                description,
                user_friendly_description,
                keywords,
                version,
                options,
                rules,
                optimized_data,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_mv_catalog() {
        let json_str = std::fs::read_to_string("mv-catalog.json").unwrap();
        let catalog: ToolCatalog = serde_json::from_str(&json_str).unwrap();

        let optimized = OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options: catalog.options.iter().map(|opt| OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: Some(OptimizedData {
                    key: "opt_custom_emb".to_string(),
                    data: vec![4, 5, 6],
                    data_type: "BLOB".to_string(),
                }),
            }).collect(),
            rules: catalog.rules.clone(),
            optimized_data: Some(OptimizedData {
                key: "tool_custom_emb".to_string(),
                data: vec![1, 2, 3],
                data_type: "BLOB".to_string(),
            }),
        };

        let adapter = PersistenceAdapter::new();
        let _ = adapter.delete_catalog(&optimized.tool_name);
        
        let saved = adapter.save_catalog(&optimized).unwrap();
        assert!(saved);

        let fetched = adapter.fetch_catalog(&optimized.tool_name).unwrap();
        assert_eq!(fetched.tool_name, optimized.tool_name);
        assert_eq!(fetched.description, optimized.description);
        assert_eq!(fetched.keywords, optimized.keywords);
        assert_eq!(fetched.options.len(), optimized.options.len());

        // Validate custom Dynamic Optimized Data is retrieved correctly
        let tool_opt_data = fetched.optimized_data.unwrap();
        assert_eq!(tool_opt_data.key, "tool_custom_emb");
        assert_eq!(tool_opt_data.data, vec![1, 2, 3]);

        for (i, opt) in fetched.options.iter().enumerate() {
            assert_eq!(opt.option_name, optimized.options[i].option_name);
            assert_eq!(opt.description, optimized.options[i].description);
            assert_eq!(opt.keywords, optimized.options[i].keywords);

            let opt_opt_data = opt.optimized_data.as_ref().unwrap();
            assert_eq!(opt_opt_data.key, "opt_custom_emb");
            assert_eq!(opt_opt_data.data, vec![4, 5, 6]);
        }
    }
}
