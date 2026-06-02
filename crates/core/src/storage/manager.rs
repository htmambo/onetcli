use super::connection::SqliteConnection;
use super::migration::run_migrations;
use super::runtime_paths::get_db_path;
use anyhow::Result;
use dashmap::DashMap;
use gpui::{App, Global};
use std::any::{Any, TypeId};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

pub struct StorageManager {
    conn: SqliteConnection,
    repositories: Arc<DashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

pub struct GlobalStorageState {
    pub storage: StorageManager,
}

impl Global for GlobalStorageState {}

impl Clone for GlobalStorageState {
    fn clone(&self) -> Self {
        GlobalStorageState {
            storage: self.storage.clone(),
        }
    }
}

impl StorageManager {
    pub fn new() -> Result<Self> {
        let db_path = get_db_path()?;
        Self::with_path(&db_path)
    }

    /// 使用指定路径创建存储管理器（用于测试隔离）
    pub fn with_path(db_path: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        let conn = SqliteConnection::open(db_path)?;

        conn.with_connection(|c| {
            run_migrations(c)?;
            Ok(())
        })?;

        let manager = Self {
            conn,
            repositories: Arc::new(DashMap::new()),
        };
        Ok(manager)
    }

    pub fn connection(&self) -> SqliteConnection {
        self.conn.clone()
    }

    pub fn register<R>(&self, repo: R)
    where
        R: 'static + Send + Sync,
    {
        let type_id = TypeId::of::<R>();
        self.repositories.insert(type_id, Arc::new(repo));
    }

    pub fn get<R>(&self) -> Option<Arc<R>>
    where
        R: 'static + Send + Sync,
    {
        self.repositories
            .get(&TypeId::of::<R>())
            .and_then(|v| v.clone().downcast::<R>().ok())
    }
}

impl Clone for StorageManager {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            repositories: Arc::clone(&self.repositories),
        }
    }
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间不应早于 UNIX 纪元")
        .as_secs() as i64
}

pub fn init(cx: &mut App) {
    let global_storage_state = match StorageManager::new() {
        Ok(manager) => GlobalStorageState { storage: manager },
        Err(err) => {
            error!("Failed to initialize storage manager: {}", err);
            panic!("Failed to initialize storage manager: {}", err);
        }
    };
    cx.set_global(global_storage_state)
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
