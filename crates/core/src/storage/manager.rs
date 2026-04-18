use super::connection::SqliteConnection;
use super::migration::run_migrations;
use anyhow::Result;
use dashmap::DashMap;
use gpui::{App, Global};
use std::any::{Any, TypeId};
use std::path::{Path, PathBuf};
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

pub fn get_db_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("one-hub.db"))
}

pub fn get_config_dir() -> Result<PathBuf> {
    let config_dir = if cfg!(target_os = "macos") {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".config")
            .join("one-hub")
    } else if cfg!(target_os = "windows") {
        dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
            .join("one-hub")
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".config")
            .join("one-hub")
    };

    Ok(config_dir)
}

pub fn get_download_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

/// Returns the user-facing themes directory.
/// Uses `~/.config/one-hub/themes` on Linux/macOS and `%APPDATA%/one-hub/themes` on Windows.
pub fn get_themes_dir() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("themes"))
}

/// 返回当前运行态应使用的主题目录。
/// 开发态（`cargo run -p main` / `target/...`）直接读取工作区 `themes/`，
/// 安装态继续读取用户配置目录。
pub fn get_runtime_themes_dir() -> Result<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dev_themes_dir) = find_workspace_themes_dir_for_exe(&exe_path) {
            return Ok(dev_themes_dir);
        }
    }

    get_themes_dir()
}

/// Copies bundled default themes to the user's themes directory if none exist.
/// This is called on first run to populate the user's theme collection.
pub fn ensure_themes_copied() -> Result<()> {
    if let Ok(exe_path) = std::env::current_exe() {
        if find_workspace_themes_dir_for_exe(&exe_path).is_some() {
            return Ok(());
        }
    }

    let themes_dir = get_themes_dir()?;
    if themes_dir.exists() {
        if theme_dir_has_json(&themes_dir) {
            return Ok(());
        }
    }

    std::fs::create_dir_all(&themes_dir)?;

    if let Some(bundled) = find_bundled_themes_dir() {
        if bundled.exists() {
            for entry in std::fs::read_dir(bundled)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let file_name = path.file_name().unwrap();
                    let dest = themes_dir.join(file_name);
                    std::fs::copy(&path, &dest)?;
                }
            }
            return Ok(());
        }
    }

    Ok(())
}

/// Finds the bundled themes directory relative to the current executable.
/// On Linux: /path/to/onetcli/../share/onetcli/themes
/// On macOS: /path/to/onetcli/../share/onetcli/themes
fn find_bundled_themes_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    // Installed: look for share/onetcli/themes relative to executable
    #[cfg(target_os = "linux")]
    let installed_path = exe_dir.join("../share/onetcli/themes");

    #[cfg(target_os = "macos")]
    let installed_path = exe_dir.join("../../share/onetcli/themes");

    #[cfg(target_os = "windows")]
    let installed_path = exe_dir.join("../share/onetcli/themes");

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let installed_path = exe_dir.join("share/onetcli/themes");

    if let Ok(installed) = installed_path.canonicalize() {
        if installed.exists() {
            return Some(installed);
        }
    }

    None
}

fn find_workspace_themes_dir_for_exe(exe_path: &Path) -> Option<PathBuf> {
    let target_dir = exe_path
        .parent()?
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("target"))?;
    let workspace_dir = target_dir.parent()?;
    let themes_dir = workspace_dir.join("themes");

    if workspace_dir.join("Cargo.toml").is_file() && theme_dir_has_json(&themes_dir) {
        return Some(themes_dir);
    }

    None
}

fn theme_dir_has_json(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
}

pub fn get_queries_dir() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    let queries_dir = config_dir.join("queries");
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }
    Ok(queries_dir)
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
