use std::collections::HashMap;

use anyhow::Result;
use rusqlite::Connection;
use rust_i18n::t;
use tracing::{error, info};

use super::get_config_dir;
use super::models::{DatabaseType, DbConnectionConfig, StoredConnection};
use super::repository::ConnectionRepository;
use super::traits::Repository;

const DEMO_SQL: &str = include_str!("demo_orders.sql");
const DEMO_DB_FILENAME: &str = "demo_orders.db";
const DEMO_CONNECTION_NAME: &str = "Demo Orders (SQLite)";

/// 首次启动时创建演示数据库。
///
/// 仅当连接数为 0 时触发，确保只在全新安装时生效。
/// 所有错误只记录日志，不阻止应用启动。
pub fn try_init_demo(repo: &ConnectionRepository) {
    let count = match repo.count() {
        Ok(c) => c,
        Err(e) => {
            error!(
                "{}",
                t!("DemoDatabase.log_check_count_failed", error = e.to_string())
            );
            return;
        }
    };

    if count > 0 {
        return;
    }

    info!("{}", t!("DemoDatabase.log_first_run_start"));

    if let Err(e) = init_demo_inner(repo) {
        error!(
            "{}",
            t!("DemoDatabase.log_create_failed", error = e.to_string())
        );
    }
}

fn init_demo_inner(repo: &ConnectionRepository) -> Result<()> {
    let config_dir = get_config_dir()?;
    let db_path = config_dir.join(DEMO_DB_FILENAME);

    if !db_path.exists() {
        create_demo_db_file(&db_path)?;
        info!(
            "{}",
            t!(
                "DemoDatabase.log_db_file_created",
                path = db_path.display().to_string()
            )
        );
    }

    register_demo_connection(repo, &db_path)?;
    info!(
        "{}",
        t!(
            "DemoDatabase.log_connection_registered",
            name = DEMO_CONNECTION_NAME
        )
    );

    Ok(())
}

/// 创建 SQLite 数据库文件并执行建表与数据插入脚本。
fn create_demo_db_file(path: &std::path::Path) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(DEMO_SQL)?;
    Ok(())
}

/// 构建 StoredConnection 并写入应用数据库。
fn register_demo_connection(repo: &ConnectionRepository, db_path: &std::path::Path) -> Result<i64> {
    let path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("{}", t!("DemoDatabase.invalid_path")))?
        .to_string();

    let config = DbConnectionConfig {
        id: String::new(),
        database_type: DatabaseType::SQLite,
        name: DEMO_CONNECTION_NAME.to_string(),
        host: path_str,
        port: 0,
        username: String::new(),
        password: String::new(),
        database: None,
        service_name: None,
        sid: None,
        workspace_id: None,
        credential_ref: None,
        ssh_tunnel_credential_ref: None,
        extra_params: HashMap::new(),
    };

    let mut stored = StoredConnection::new_database(DEMO_CONNECTION_NAME.to_string(), config, None);
    stored.sync_enabled = false;

    let id = repo.insert(&mut stored)?;
    Ok(id)
}
