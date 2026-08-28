use anyhow::Result;
use rusqlite::Connection;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "20260225000001",
        include_str!("../../migrations/20260225000001_init.sql"),
    ),
    ("20260315000001", ""),
    (
        "20260317000001",
        include_str!("../../migrations/20260317000001_connection_owner.sql"),
    ),
    (
        "20260325000001",
        include_str!("../../migrations/20260325000001_certificate_management.sql"),
    ),
    (
        "20260326000001",
        include_str!("../../migrations/20260326000001_workspace_sync_state.sql"),
    ),
    (
        "20260326000002",
        include_str!("../../migrations/20260326000002_workspace_delete_context.sql"),
    ),
    (
        "20260326000003",
        include_str!("../../migrations/20260326000003_home_manual_sort.sql"),
    ),
    (
        "20260408000001",
        include_str!("../../migrations/20260408000001_llm_thinking_budget.sql"),
    ),
    (
        "20260410000001",
        include_str!("../../migrations/20260410000001_certificate_params.sql"),
    ),
    (
        "20260410000002",
        include_str!("../../migrations/20260410000002_key_value.sql"),
    ),
    (
        "20260418000001",
        include_str!("../../migrations/20260418000001_drop_old_certificate_columns.sql"),
    ),
    (
        "20260502000001",
        include_str!("../../migrations/20260502000001_chat_session_connection.sql"),
    ),
    (
        "20260516000001",
        include_str!("../../migrations/20260516000001_chat_session_schema.sql"),
    ),
    (
        "20260517000001",
        include_str!("../../migrations/20260517000001_llm_provider_sync.sql"),
    ),
    (
        "20260610000001",
        include_str!("../../migrations/20260610000001_sftp_favorite_paths.sql"),
    ),
    (
        "20260624000001",
        include_str!("../../migrations/20260624000001_chat_message_tool_calls.sql"),
    ),
];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;

    for (version, sql) in MIGRATIONS {
        let applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM _migrations WHERE version = ?1",
            [version],
            |row| row.get(0),
        )?;

        if applied == 0 {
            if let Err(e) = conn.execute_batch(sql) {
                let err_msg = e.to_string();
                if err_msg.contains("duplicate column name") || err_msg.contains("no such column") {
                    tracing::warn!(
                        "Migration {} skipped (column already exists or removed): {}",
                        version,
                        err_msg
                    );
                } else {
                    return Err(e.into());
                }
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO _migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![version, now],
            )?;
        }
    }

    Ok(())
}
