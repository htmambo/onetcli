use super::StorageManager;
use crate::storage::{get_config_dir, get_db_path, get_queries_dir, get_themes_dir};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn home_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct HomeDirGuard {
    previous_home: Option<OsString>,
    temp_home: PathBuf,
}

impl HomeDirGuard {
    fn new(temp_home: PathBuf) -> Self {
        let previous_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }
        Self {
            previous_home,
            temp_home,
        }
    }
}

impl Drop for HomeDirGuard {
    fn drop(&mut self) {
        if let Some(previous_home) = self.previous_home.as_ref() {
            unsafe {
                std::env::set_var("HOME", previous_home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        let _ = std::fs::remove_dir_all(&self.temp_home);
    }
}

fn with_temp_home() -> HomeDirGuard {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间异常")
        .as_nanos();
    let temp_home = std::env::temp_dir().join(format!(
        "omnihub-storage-manager-{}-{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&temp_home).expect("创建临时 HOME 目录失败");
    HomeDirGuard::new(temp_home)
}

#[test]
fn storage_manager_with_path_creates_database_file() {
    let temp_dir = tempfile::tempdir().expect("应创建临时目录");
    let db_path = temp_dir.path().join("nested").join("omnihub.db");

    let manager = StorageManager::with_path(&db_path).expect("应创建 StorageManager");

    assert!(db_path.exists());
    assert!(manager.connection().with_connection(|_| Ok(())).is_ok());
}

#[test]
fn runtime_paths_follow_home_config_directory() {
    let _guard = home_env_lock().lock().expect("获取 HOME 环境锁失败");
    let home_guard = with_temp_home();

    let expected_config_dir = home_guard.temp_home.join(".config").join("omnihub");
    assert_eq!(
        get_config_dir().expect("应返回配置目录"),
        expected_config_dir
    );
    assert_eq!(
        get_db_path().expect("应返回数据库路径"),
        expected_config_dir.join("omnihub.db")
    );
    assert_eq!(
        get_themes_dir().expect("应返回主题目录"),
        expected_config_dir.join("themes")
    );

    let queries_dir = get_queries_dir().expect("应创建 queries 目录");
    assert_eq!(queries_dir, expected_config_dir.join("queries"));
    assert!(queries_dir.exists());
}
