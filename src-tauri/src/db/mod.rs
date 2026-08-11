//! SQLite access layer. Uses sqlx with a single shared pool.
//!
//! Keep this module focused on schema + low-level queries. Higher-level
//! transactions live in `pipeline/*` and `ingest/*`.

pub mod models;
pub mod queries;

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

pub use models::*;

#[derive(Clone)]
pub struct Db {
    pub pool: Pool<Sqlite>,
}

impl Db {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        // 4 connections is plenty: pipeline stages run sequentially per
        // job, and the UI fires only a handful of read queries on each
        // navigation. Each extra idle connection holds a non-trivial
        // amount of cache.
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("opening sqlite pool")?;

        Ok(Self { pool })
    }

    /// Runs the embedded migration SQL files.
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        // Migrations live next to src-tauri/migrations/*.sql. We use sqlx
        // "manual" execution rather than the macro so we don't require
        // DATABASE_URL at compile time for a new-clone build.
        let migration_sql: &[(&str, &str)] = &[
            ("001_init", include_str!("../../migrations/001_init.sql")),
            ("002_srs", include_str!("../../migrations/002_srs.sql")),
            (
                "003_fts_triggers",
                include_str!("../../migrations/003_fts_triggers.sql"),
            ),
            (
                "004_experiments",
                include_str!("../../migrations/004_experiments.sql"),
            ),
            (
                "005_comparisons_and_levels",
                include_str!("../../migrations/005_comparisons_and_levels.sql"),
            ),
        ];

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _smilie_migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        for (name, sql) in migration_sql {
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT name FROM _smilie_migrations WHERE name = ?1")
                    .bind(name)
                    .fetch_optional(&self.pool)
                    .await?;
            if existing.is_some() {
                continue;
            }

            // Execute multi-statement migration.
            let mut conn = self.pool.acquire().await?;
            for stmt in split_sql(sql) {
                let trimmed = stmt.trim();
                if trimmed.is_empty() {
                    continue;
                }
                sqlx::query(trimmed)
                    .execute(&mut *conn)
                    .await
                    .with_context(|| {
                        format!("migration {name} stmt: {}", truncate(trimmed, 120))
                    })?;
            }
            sqlx::query("INSERT INTO _smilie_migrations (name, applied_at) VALUES (?1, ?2)")
                .bind(name)
                .bind(chrono::Utc::now().timestamp())
                .execute(&self.pool)
                .await?;

            tracing::info!(%name, "applied migration");
        }

        Ok(())
    }
}

/// SQL splitter for our migration files. Handles plain `;` terminators
/// and treats `BEGIN ... END;` blocks (used by triggers) as a single
/// statement so the inner semicolons don't fragment the trigger body.
/// Line comments (`-- ...`) are stripped before scanning so a `;`
/// inside a comment never starts a new statement.
fn split_sql(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_block = false;
    for raw_line in src.lines() {
        // Strip line comments — they may contain `;` characters that
        // would otherwise be treated as statement terminators.
        let line = match raw_line.find("--") {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        };
        let upper = line.to_uppercase();
        if !in_block && upper.contains("BEGIN") {
            in_block = true;
        }
        current.push_str(line);
        current.push('\n');
        if in_block {
            // A line containing `END;` (case-insensitive) closes the block.
            if upper.contains("END") && line.trim_end().ends_with(';') {
                in_block = false;
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        // Outside a block — split on `;`.
        while let Some(idx) = current.find(';') {
            let stmt: String = current.drain(..=idx).collect();
            out.push(stmt);
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pages_fts` is contentless, so the FK cascade on `pages` does not
    /// touch it. Migration 003 adds an `AFTER DELETE` trigger that wipes
    /// the matching FTS row by `page_id`. Without this trigger we orphan
    /// FTS rows on every paper-delete or `clear_pages_for_paper` call,
    /// and a future `search_pages_fts` returns dead page ids.
    #[tokio::test]
    async fn fts_row_is_removed_when_page_is_deleted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("smilie.db");
        let db = Db::open(&db_path).await.expect("open db");
        db.run_migrations().await.expect("run migrations");

        // Insert a paper + page so the FK cascade target exists.
        let paper_id = queries::insert_paper(&db, "P", "[]", "/x", None)
            .await
            .expect("insert paper");
        let page_id = queries::insert_page(&db, &paper_id, "T", "concept", "[]", "[]", 0)
            .await
            .expect("insert page");
        queries::index_page_fts(&db, &page_id, &paper_id, "T", "bagel torus")
            .await
            .expect("index fts");

        // Sanity check: the FTS row exists.
        let pre: (i64,) = sqlx::query_as("SELECT count(*) FROM pages_fts WHERE page_id = ?1")
            .bind(&page_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(pre.0, 1, "pre-delete FTS row should exist");

        // Path 1: clear_pages_for_paper (called by build_learning_plan).
        queries::clear_pages_for_paper(&db, &paper_id)
            .await
            .expect("clear pages");

        let post: (i64,) = sqlx::query_as("SELECT count(*) FROM pages_fts WHERE page_id = ?1")
            .bind(&page_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            post.0, 0,
            "AFTER DELETE trigger should remove the FTS row when its page is deleted"
        );

        // Path 2: delete_paper — exercise the cascade indirectly.
        let page_id_2 = queries::insert_page(&db, &paper_id, "T2", "concept", "[]", "[]", 0)
            .await
            .expect("insert page 2");
        queries::index_page_fts(&db, &page_id_2, &paper_id, "T2", "more text")
            .await
            .expect("index fts 2");
        queries::delete_paper(&db, &paper_id)
            .await
            .expect("delete paper");
        let post2: (i64,) = sqlx::query_as("SELECT count(*) FROM pages_fts WHERE page_id = ?1")
            .bind(&page_id_2)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            post2.0, 0,
            "delete_paper should cascade through pages and trigger cleans up FTS"
        );
    }
}
