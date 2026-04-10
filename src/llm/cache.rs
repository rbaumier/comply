//! SQLite cache for LLM rule results.
//!
//! Key: `sha256(rule_id + rule_version + block_content)`.
//! Value: serialized `Vec<Diagnostic>` as JSON.
//! No TTL — invalidation is purely hash-based (content change or
//! rule_version bump = new key = cache miss).

use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Open (or create) the cache database at `.comply/cache.db` relative
/// to `project_root`. Creates the `.comply/` directory if needed.
pub fn open(project_root: &Path) -> Result<Connection> {
    let dir = project_root.join(".comply");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;
    let db_path = dir.join("cache.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open cache at {}", db_path.display()))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS llm_cache (
            cache_key TEXT PRIMARY KEY,
            diagnostics_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    )
    .context("failed to create llm_cache table")?;
    Ok(conn)
}

/// Build a deterministic cache key from rule identity + content.
pub fn cache_key(rule_id: &str, rule_version: u32, block_content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rule_id.as_bytes());
    hasher.update(rule_version.to_le_bytes());
    hasher.update(block_content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Look up a cached result. Returns the raw JSON string if found.
pub fn lookup(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT diagnostics_json FROM llm_cache WHERE cache_key = ?1",
        [key],
        |row| row.get(0),
    )
    .ok()
}

/// Store a result in the cache.
pub fn store(conn: &Connection, key: &str, diagnostics_json: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    conn.execute(
        "INSERT OR REPLACE INTO llm_cache (cache_key, diagnostics_json, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![key, diagnostics_json, now as i64],
    )
    .context("failed to store in llm_cache")?;
    Ok(())
}

/// Return the cache database path for display purposes.
pub fn db_path(project_root: &Path) -> PathBuf {
    project_root.join(".comply").join("cache.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip() {
        let tmp = TempDir::new().unwrap();
        let conn = open(tmp.path()).unwrap();
        let key = cache_key("test-rule", 1, "fn foo() {}");
        assert!(lookup(&conn, &key).is_none());
        store(&conn, &key, "[]").unwrap();
        assert_eq!(lookup(&conn, &key), Some("[]".to_string()));
    }

    #[test]
    fn version_bump_invalidates() {
        let k1 = cache_key("r", 1, "content");
        let k2 = cache_key("r", 2, "content");
        assert_ne!(k1, k2);
    }

    #[test]
    fn content_change_invalidates() {
        let k1 = cache_key("r", 1, "fn a() {}");
        let k2 = cache_key("r", 1, "fn b() {}");
        assert_ne!(k1, k2);
    }
}
