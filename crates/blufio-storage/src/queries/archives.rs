// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compaction archive CRUD operations.

use blufio_core::BlufioError;
use rusqlite::params;

use crate::database::Database;

/// A row from the `compaction_archives` table.
#[derive(Debug, Clone)]
pub struct ArchiveRow {
    /// Unique archive identifier.
    pub id: String,
    /// User that owns this archive.
    pub user_id: String,
    /// Compaction summary text.
    pub summary: String,
    /// Quality score of the summary (if scored).
    pub quality_score: Option<f64>,
    /// JSON array of session IDs that contributed to this archive.
    pub session_ids: String,
    /// Data classification level.
    pub classification: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// Token count of the summary.
    pub token_count: Option<i64>,
}

/// Convert a rusqlite Row to an ArchiveRow.
///
/// Column order: id(0), user_id(1), summary(2), quality_score(3),
/// session_ids(4), classification(5), created_at(6), token_count(7).
fn row_to_archive(row: &rusqlite::Row) -> ArchiveRow {
    ArchiveRow {
        id: row.get(0).unwrap_or_default(),
        user_id: row.get(1).unwrap_or_default(),
        summary: row.get(2).unwrap_or_default(),
        quality_score: row.get(3).unwrap_or_default(),
        session_ids: row.get(4).unwrap_or_default(),
        classification: row.get(5).unwrap_or_default(),
        created_at: row.get(6).unwrap_or_default(),
        token_count: row.get(7).unwrap_or_default(),
    }
}

/// Insert a new compaction archive.
#[allow(clippy::too_many_arguments)]
pub async fn insert_archive(
    db: &Database,
    id: &str,
    user_id: &str,
    summary: &str,
    quality_score: Option<f64>,
    session_ids: &str,
    classification: &str,
    created_at: &str,
    token_count: Option<i64>,
) -> Result<(), BlufioError> {
    let id = id.to_string();
    let user_id = user_id.to_string();
    let summary = summary.to_string();
    let session_ids = session_ids.to_string();
    let classification = classification.to_string();
    let created_at = created_at.to_string();

    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO compaction_archives (id, user_id, summary, quality_score, session_ids, classification, created_at, token_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, user_id, summary, quality_score, session_ids, classification, created_at, token_count],
            )?;
            Ok(())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// List archives for a user, ordered by most recent first.
pub async fn list_archives(
    db: &Database,
    user_id: &str,
    limit: i64,
) -> Result<Vec<ArchiveRow>, BlufioError> {
    let user_id = user_id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, summary, quality_score, session_ids, classification, created_at, token_count
                 FROM compaction_archives WHERE user_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![user_id, limit], |row| Ok(row_to_archive(row)))?;
            let mut archives = Vec::new();
            for row in rows {
                archives.push(row?);
            }
            Ok(archives)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// List all archives across all users, ordered by most recent first.
pub async fn list_all_archives(
    db: &Database,
    limit: i64,
) -> Result<Vec<ArchiveRow>, BlufioError> {
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, summary, quality_score, session_ids, classification, created_at, token_count
                 FROM compaction_archives
                 ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| Ok(row_to_archive(row)))?;
            let mut archives = Vec::new();
            for row in rows {
                archives.push(row?);
            }
            Ok(archives)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Get a single archive by ID.
pub async fn get_archive(
    db: &Database,
    id: &str,
) -> Result<Option<ArchiveRow>, BlufioError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, summary, quality_score, session_ids, classification, created_at, token_count
                 FROM compaction_archives WHERE id = ?1",
            )?;
            let result = stmt.query_row(params![id], |row| Ok(row_to_archive(row)));
            match result {
                Ok(archive) => Ok(Some(archive)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Delete an archive by ID. Returns true if a row was deleted.
pub async fn delete_archive(
    db: &Database,
    id: &str,
) -> Result<bool, BlufioError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let affected = conn.execute(
                "DELETE FROM compaction_archives WHERE id = ?1",
                params![id],
            )?;
            Ok(affected > 0)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Count archives for a user.
pub async fn count_archives(
    db: &Database,
    user_id: &str,
) -> Result<i64, BlufioError> {
    let user_id = user_id.to_string();
    db.connection()
        .call(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM compaction_archives WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Get the oldest archive for a user.
pub async fn oldest_archive(
    db: &Database,
    user_id: &str,
) -> Result<Option<ArchiveRow>, BlufioError> {
    let user_id = user_id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, summary, quality_score, session_ids, classification, created_at, token_count
                 FROM compaction_archives WHERE user_id = ?1
                 ORDER BY created_at ASC LIMIT 1",
            )?;
            let result = stmt.query_row(params![user_id], |row| Ok(row_to_archive(row)));
            match result {
                Ok(archive) => Ok(Some(archive)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Delete archives that reference a specific session ID (for GDPR erasure).
///
/// Uses LIKE matching against the JSON session_ids array string.
pub async fn delete_archives_by_session_ids(
    db: &Database,
    session_id_contains: &str,
) -> Result<usize, BlufioError> {
    let pattern = format!("%{session_id_contains}%");
    db.connection()
        .call(move |conn| {
            let affected = conn.execute(
                "DELETE FROM compaction_archives WHERE session_ids LIKE ?1",
                params![pattern],
            )?;
            Ok(affected)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn setup_db() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_archives.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn insert_and_get_archive() {
        let (db, _dir) = setup_db().await;

        insert_archive(
            &db,
            "arc-1",
            "user-1",
            "Summary of conversation",
            Some(0.85),
            r#"["sess-1","sess-2"]"#,
            "internal",
            "2026-01-01T00:00:00Z",
            Some(256),
        )
        .await
        .unwrap();

        let arc = get_archive(&db, "arc-1").await.unwrap();
        assert!(arc.is_some());
        let arc = arc.unwrap();
        assert_eq!(arc.id, "arc-1");
        assert_eq!(arc.user_id, "user-1");
        assert_eq!(arc.summary, "Summary of conversation");
        assert_eq!(arc.quality_score, Some(0.85));
        assert_eq!(arc.session_ids, r#"["sess-1","sess-2"]"#);
        assert_eq!(arc.classification, "internal");
        assert_eq!(arc.token_count, Some(256));
    }

    #[tokio::test]
    async fn list_archives_ordered_by_newest() {
        let (db, _dir) = setup_db().await;

        for i in 0..3 {
            insert_archive(
                &db,
                &format!("arc-{i}"),
                "user-1",
                &format!("Summary {i}"),
                None,
                "[]",
                "internal",
                &format!("2026-01-0{i}T00:00:00Z", i = i + 1),
                None,
            )
            .await
            .unwrap();
        }

        let archives = list_archives(&db, "user-1", 10).await.unwrap();
        assert_eq!(archives.len(), 3);
        // Most recent first
        assert_eq!(archives[0].id, "arc-2");
        assert_eq!(archives[2].id, "arc-0");
    }

    #[tokio::test]
    async fn list_archives_respects_limit() {
        let (db, _dir) = setup_db().await;

        for i in 0..5 {
            insert_archive(
                &db,
                &format!("arc-{i}"),
                "user-1",
                &format!("Summary {i}"),
                None,
                "[]",
                "internal",
                &format!("2026-01-0{i}T00:00:00Z", i = i + 1),
                None,
            )
            .await
            .unwrap();
        }

        let archives = list_archives(&db, "user-1", 2).await.unwrap();
        assert_eq!(archives.len(), 2);
    }

    #[tokio::test]
    async fn delete_archive_returns_true_when_found() {
        let (db, _dir) = setup_db().await;

        insert_archive(
            &db,
            "arc-del",
            "user-1",
            "To be deleted",
            None,
            "[]",
            "internal",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await
        .unwrap();

        assert!(delete_archive(&db, "arc-del").await.unwrap());
        assert!(!delete_archive(&db, "arc-del").await.unwrap());
    }

    #[tokio::test]
    async fn count_archives_for_user() {
        let (db, _dir) = setup_db().await;

        for i in 0..3 {
            insert_archive(
                &db,
                &format!("arc-{i}"),
                "user-1",
                &format!("Summary {i}"),
                None,
                "[]",
                "internal",
                "2026-01-01T00:00:00Z",
                None,
            )
            .await
            .unwrap();
        }

        assert_eq!(count_archives(&db, "user-1").await.unwrap(), 3);
        assert_eq!(count_archives(&db, "user-2").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn oldest_archive_returns_earliest() {
        let (db, _dir) = setup_db().await;

        insert_archive(
            &db,
            "arc-new",
            "user-1",
            "Newer",
            None,
            "[]",
            "internal",
            "2026-02-01T00:00:00Z",
            None,
        )
        .await
        .unwrap();
        insert_archive(
            &db,
            "arc-old",
            "user-1",
            "Older",
            None,
            "[]",
            "internal",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await
        .unwrap();

        let oldest = oldest_archive(&db, "user-1").await.unwrap();
        assert!(oldest.is_some());
        assert_eq!(oldest.unwrap().id, "arc-old");
    }

    #[tokio::test]
    async fn delete_archives_by_session_ids_gdpr() {
        let (db, _dir) = setup_db().await;

        insert_archive(
            &db,
            "arc-1",
            "user-1",
            "Has target session",
            None,
            r#"["sess-target","sess-2"]"#,
            "internal",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await
        .unwrap();
        insert_archive(
            &db,
            "arc-2",
            "user-1",
            "No target session",
            None,
            r#"["sess-3"]"#,
            "internal",
            "2026-01-02T00:00:00Z",
            None,
        )
        .await
        .unwrap();

        let deleted = delete_archives_by_session_ids(&db, "sess-target")
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        // arc-2 should still exist
        assert!(get_archive(&db, "arc-2").await.unwrap().is_some());
        assert!(get_archive(&db, "arc-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_nonexistent_archive_returns_none() {
        let (db, _dir) = setup_db().await;
        assert!(get_archive(&db, "nonexistent").await.unwrap().is_none());
    }
}
