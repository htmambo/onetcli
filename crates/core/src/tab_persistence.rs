use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use gpui::{App, AsyncApp, Entity, Task, Window};

use crate::connection_restore::{save_connection_restore_snapshot, snapshot_from_tab_state};
use crate::storage::get_config_dir;
use crate::tab_container::{TabContainer, TabContainerState, TabContentRegistry};

const TAB_STATE_FILE: &str = "tab_state.json";
const SAVE_DELAY_SECS: u64 = 5;

fn get_tab_state_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }
    Ok(config_dir.join(TAB_STATE_FILE))
}

pub fn save_tab_state(state: &TabContainerState) -> Result<()> {
    let path = get_tab_state_path()?;
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(&path, json)?;
    let snapshot = snapshot_from_tab_state(state);
    save_connection_restore_snapshot(&snapshot)?;
    tracing::info!("Tab state saved to {:?}", path);
    Ok(())
}

pub fn load_tab_state() -> Result<TabContainerState> {
    let path = get_tab_state_path()?;
    if !path.exists() {
        return Ok(TabContainerState::default());
    }
    let json = std::fs::read_to_string(&path).context("Failed to read tab state file")?;
    let state = serde_json::from_str(&json).context("Failed to parse tab state JSON")?;
    tracing::info!("Tab state loaded from {:?}", path);
    Ok(state)
}

pub fn tab_state_exists() -> bool {
    get_tab_state_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn load_tabs(
    tab_container: &Entity<TabContainer>,
    registry: &TabContentRegistry,
    window: &mut Window,
    cx: &mut App,
) -> Result<()> {
    let state = load_tab_state()?;

    if state.tabs.is_empty() {
        tracing::info!("Saved tab state is empty");
        return Ok(());
    }

    tab_container.update(cx, |container, cx| {
        container.load(state, registry, window, cx);
    });

    tracing::info!("Tabs restored from saved state");
    Ok(())
}

pub fn schedule_save(
    tab_container: Entity<TabContainer>,
    last_layout_state: &mut Option<TabContainerState>,
    cx: &mut App,
) -> Task<()> {
    let last_state = last_layout_state.clone();

    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor()
            .timer(Duration::from_secs(SAVE_DELAY_SECS))
            .await;

        if let Some(t) = cx.update(move |cx| {
            let current_state = tab_container.read(cx).dump(cx);

            if Some(&current_state) == last_state.as_ref() {
                tracing::debug!("Tab state unchanged, skipping save");
                return None;
            }

            if let Err(err) = save_tab_state(&current_state) {
                tracing::error!("Failed to save tab state: {:?}", err);
            }

            Some(current_state)
        }) {
            tracing::info!("Tab state saved, {:?}", t)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::connection_restore::load_connection_restore_snapshot;
    use crate::tab_container::{TabContainerConfig, TabItemState};

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

    #[test]
    fn 保存标签状态时同步写入连接恢复快照() {
        let _guard = home_env_lock().lock().expect("获取 HOME 环境锁失败");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间异常")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!(
            "onetcli-tab-persistence-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&temp_home).expect("创建临时 HOME 目录失败");
        let _home_guard = HomeDirGuard::new(temp_home.clone());

        let state = TabContainerState {
            version: Some(1),
            tabs: vec![
                TabItemState {
                    id: "ssh-terminal-42-1".into(),
                    from: "ssh".into(),
                    key: "Terminal".into(),
                    data: serde_json::json!({
                        "kind": "ssh_terminal",
                        "connection_id": 42,
                        "title": "服务器"
                    }),
                },
                TabItemState {
                    id: "settings".into(),
                    from: "home".into(),
                    key: "Settings".into(),
                    data: serde_json::Value::Null,
                },
            ],
            active_index: 0,
            config: TabContainerConfig::default(),
        };

        save_tab_state(&state).expect("保存标签状态失败");

        let config_dir = temp_home.join(".config").join("one-hub");
        assert!(config_dir.join("tab_state.json").exists());
        assert!(config_dir.join("connection_restore_state.json").exists());

        let snapshot = load_connection_restore_snapshot().expect("读取连接恢复快照失败");
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].snapshot_id, "ssh-terminal-42-1");
        assert_eq!(snapshot.items[0].connection_id, Some(42));
    }
}
