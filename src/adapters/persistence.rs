use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::{ToolCatalog, CatalogMaintainer, EndUserConfig, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData, CommandRules};

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

// --- Dynamic Columns & Schema Helpers ---

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

// Ensures custom dynamic columns exist for both parent catalog and child options
fn ensure_custom_columns_exist(conn: &rusqlite::Connection, catalog: &OptimizedToolCatalog) -> Result<(), AppError> {
    if let Some(ref opt_data) = catalog.optimized_data {
        add_column_if_not_exists(conn, "tool_catalogs", &opt_data.key, &opt_data.data_type)?;
    }
    for option in &catalog.options {
        if let Some(ref opt_data) = option.optimized_data {
            add_column_if_not_exists(conn, "command_options", &opt_data.key, &opt_data.data_type)?;
        }
    }
    Ok(())
}

// --- Dynamic Query Execution Helpers ---

// Helper to insert command options of an optimized catalog
fn insert_catalog_options(conn: &rusqlite::Connection, catalog: &OptimizedToolCatalog) -> Result<(), AppError> {
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

        conn.execute(&option_sql, option_params).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    Ok(())
}

// Helper to fetch options of a tool
fn fetch_catalog_options(conn: &rusqlite::Connection, tool_name: &str) -> Result<Vec<OptimizedCommandOption>, AppError> {
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
    Ok(options)
}

// Helper to map a database row to an OptimizedToolCatalog
fn map_row_to_catalog(row: &rusqlite::Row, col_names: &[&str], options: Vec<OptimizedCommandOption>) -> Result<OptimizedToolCatalog, AppError> {
    let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
    let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
    let user_friendly_description: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
    let version: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
    let rules_json: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
    let keywords: String = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;

    let rules: CommandRules = serde_json::from_str(&rules_json)
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let optimized_data = extract_optimized_data(row, col_names, STANDARD_TOOL_COLS);

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
}

// --- StoragePort Implementation ---

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

        // Schema verification & Custom column creation
        ensure_custom_columns_exist(&tx, catalog)?;

        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

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
        insert_catalog_options(&tx, catalog)?;

        tx.commit().map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(true)
    }

    fn update_catalog(&self, catalog: &OptimizedToolCatalog) -> Result<bool, AppError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| AppError::Storage(e.to_string()))?;

        // Schema verification & Custom column creation
        ensure_custom_columns_exist(&tx, catalog)?;

        let rules_json = serde_json::to_string(&catalog.rules)
            .map_err(|e| AppError::Storage(e.to_string()))?;

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
        insert_catalog_options(&tx, catalog)?;

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
        
        // Fetch parent tool catalog info
        let mut parent_stmt = conn.prepare(
            "SELECT * FROM tool_catalogs WHERE tool_name = ?"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        let col_names: Vec<String> = parent_stmt.column_names().iter().map(|s| s.to_string()).collect();
        let col_names_refs: Vec<&str> = col_names.iter().map(|s| s.as_str()).collect();

        let mut parent_rows = parent_stmt.query(rusqlite::params![tool_name])
            .map_err(|e| AppError::Storage(e.to_string()))?;

        if let Some(row) = parent_rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
            // Fetch children options
            let options = fetch_catalog_options(&conn, tool_name)?;
            map_row_to_catalog(row, &col_names_refs, options)
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
            let options = fetch_catalog_options(&conn, &tool_name)?;
            catalogs.push(map_row_to_catalog(row, &col_names_refs, options)?);
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

    #[test]
    fn test_duplicate_catalog_error() {
        let adapter = PersistenceAdapter::new();
        let tool_name = "test_dup_tool";
        let _ = adapter.delete_catalog(tool_name);

        let catalog = OptimizedToolCatalog {
            tool_name: tool_name.to_string(),
            description: "desc".to_string(),
            user_friendly_description: "user desc".to_string(),
            keywords: "keys".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
            optimized_data: None,
        };

        // First save should succeed
        assert!(adapter.save_catalog(&catalog).is_ok());

        // Second save of same tool name should return DuplicateCatalog
        let res = adapter.save_catalog(&catalog);
        assert!(res.is_err());
        assert!(matches!(res.err().unwrap(), AppError::DuplicateCatalog(_)));

        // Clean up
        let _ = adapter.delete_catalog(tool_name);
    }

    #[test]
    fn test_catalog_not_found_errors() {
        let adapter = PersistenceAdapter::new();
        let tool_name = "non_existent_tool_12345";
        let _ = adapter.delete_catalog(tool_name); // ensure it's not there

        // Fetching should return CatalogNotFound
        let res_fetch = adapter.fetch_catalog(tool_name);
        assert!(res_fetch.is_err());
        assert!(matches!(res_fetch.err().unwrap(), AppError::CatalogNotFound(_)));

        // Updating should return CatalogNotFound
        let catalog = OptimizedToolCatalog {
            tool_name: tool_name.to_string(),
            description: "desc".to_string(),
            user_friendly_description: "".to_string(),
            keywords: "".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
            optimized_data: None,
        };
        let res_update = adapter.update_catalog(&catalog);
        assert!(res_update.is_err());
        assert!(matches!(res_update.err().unwrap(), AppError::CatalogNotFound(_)));

        // Deleting should return CatalogNotFound
        let res_delete = adapter.delete_catalog(tool_name);
        assert!(res_delete.is_err());
        assert!(matches!(res_delete.err().unwrap(), AppError::CatalogNotFound(_)));
    }

    #[test]
    fn test_update_catalog() {
        let adapter = PersistenceAdapter::new();
        let tool_name = "test_update_tool";
        let _ = adapter.delete_catalog(tool_name);

        let catalog = OptimizedToolCatalog {
            tool_name: tool_name.to_string(),
            description: "original desc".to_string(),
            user_friendly_description: "orig user desc".to_string(),
            keywords: "orig key".to_string(),
            version: "1.0".to_string(),
            options: vec![OptimizedCommandOption {
                option_name: "-v".to_string(),
                description: "verbose".to_string(),
                user_friendly_description: "verbose user".to_string(),
                keywords: "verbose".to_string(),
                optimized_data: None,
            }],
            rules: CommandRules(serde_json::json!({})),
            optimized_data: None,
        };

        // Save first
        adapter.save_catalog(&catalog).unwrap();

        // Prepare updated catalog
        let updated_catalog = OptimizedToolCatalog {
            tool_name: tool_name.to_string(),
            description: "updated desc".to_string(),
            user_friendly_description: "updated user desc".to_string(),
            keywords: "updated key".to_string(),
            version: "2.0".to_string(),
            options: vec![
                OptimizedCommandOption {
                    option_name: "-v".to_string(),
                    description: "verbose updated".to_string(),
                    user_friendly_description: "verbose user updated".to_string(),
                    keywords: "verbose".to_string(),
                    optimized_data: None,
                },
                OptimizedCommandOption {
                    option_name: "-h".to_string(),
                    description: "help".to_string(),
                    user_friendly_description: "help user".to_string(),
                    keywords: "help".to_string(),
                    optimized_data: None,
                },
            ],
            rules: CommandRules(serde_json::json!({"test": true})),
            optimized_data: None,
        };

        // Update
        let updated = adapter.update_catalog(&updated_catalog).unwrap();
        assert!(updated);

        // Fetch and assert
        let fetched = adapter.fetch_catalog(tool_name).unwrap();
        assert_eq!(fetched.description, "updated desc");
        assert_eq!(fetched.user_friendly_description, "updated user desc");
        assert_eq!(fetched.version, "2.0");
        assert_eq!(fetched.options.len(), 2);
        assert_eq!(fetched.options[0].option_name, "-v");
        assert_eq!(fetched.options[0].description, "verbose updated");
        assert_eq!(fetched.options[1].option_name, "-h");
        assert_eq!(fetched.options[1].description, "help");

        // Clean up
        let _ = adapter.delete_catalog(tool_name);
    }

    #[test]
    fn test_fetch_all_catalogs() {
        let adapter = PersistenceAdapter::new();
        let tool1 = "all_tool_1";
        let tool2 = "all_tool_2";
        let _ = adapter.delete_catalog(tool1);
        let _ = adapter.delete_catalog(tool2);

        let cat1 = OptimizedToolCatalog {
            tool_name: tool1.to_string(),
            description: "desc1".to_string(),
            user_friendly_description: "".to_string(),
            keywords: "".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
            optimized_data: None,
        };
        let cat2 = OptimizedToolCatalog {
            tool_name: tool2.to_string(),
            description: "desc2".to_string(),
            user_friendly_description: "".to_string(),
            keywords: "".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
            optimized_data: None,
        };

        adapter.save_catalog(&cat1).unwrap();
        adapter.save_catalog(&cat2).unwrap();

        let all = adapter.fetch_all_catalogs().unwrap();
        let names: Vec<String> = all.iter().map(|c| c.tool_name.clone()).collect();
        assert!(names.contains(&tool1.to_string()));
        assert!(names.contains(&tool2.to_string()));

        // Clean up
        let _ = adapter.delete_catalog(tool1);
        let _ = adapter.delete_catalog(tool2);
    }

    #[test]
    fn test_maintainer_lifecycle() {
        let adapter = PersistenceAdapter::new();
        let m_id = "test_maintainer_id";
        let _ = adapter.delete_maintainer(m_id);

        let maintainer = CatalogMaintainer {
            id: m_id.to_string(),
            name: "John Doe".to_string(),
            auth_key: "secure_key".to_string(),
        };

        // 1. Save
        assert!(adapter.save_maintainer(&maintainer).unwrap());

        // 2. Fetch
        let fetched = adapter.fetch_maintainer(m_id).unwrap();
        assert_eq!(fetched.name, "John Doe");
        assert_eq!(fetched.auth_key, "secure_key");

        // 3. Update
        let updated_m = CatalogMaintainer {
            id: m_id.to_string(),
            name: "Jane Doe".to_string(),
            auth_key: "new_secure_key".to_string(),
        };
        assert!(adapter.update_maintainer(&updated_m).unwrap());

        let fetched_updated = adapter.fetch_maintainer(m_id).unwrap();
        assert_eq!(fetched_updated.name, "Jane Doe");
        assert_eq!(fetched_updated.auth_key, "new_secure_key");

        // 4. Delete
        assert!(adapter.delete_maintainer(m_id).unwrap());

        // 5. Fetch non-existent should fail
        let res_fetch = adapter.fetch_maintainer(m_id);
        assert!(res_fetch.is_err());
        assert!(matches!(res_fetch.err().unwrap(), AppError::MaintainerNotFound(_)));

        // 6. Delete non-existent should fail
        let res_delete = adapter.delete_maintainer(m_id);
        assert!(res_delete.is_err());
        assert!(matches!(res_delete.err().unwrap(), AppError::MaintainerNotFound(_)));

        // 7. Update non-existent should fail
        let res_update = adapter.update_maintainer(&updated_m);
        assert!(res_update.is_err());
        assert!(matches!(res_update.err().unwrap(), AppError::MaintainerNotFound(_)));
    }

    #[test]
    fn test_configuration_lifecycle() {
        let adapter = PersistenceAdapter::new();

        // Load original configuration
        let original_config = adapter.load_configuration().unwrap();

        // Save new configuration with opposite logging_opt_in status
        let new_config = EndUserConfig {
            logging_opt_in: !original_config.logging_opt_in,
        };
        assert!(adapter.save_configuration(&new_config).unwrap());

        // Load and assert
        let fetched_config = adapter.load_configuration().unwrap();
        assert_eq!(fetched_config.logging_opt_in, !original_config.logging_opt_in);

        // Restore original config
        assert!(adapter.save_configuration(&original_config).unwrap());
    }
}
