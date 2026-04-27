use std::path::PathBuf;

use anyhow::Result;
use gpui::{Action, App, SharedString};
use gpui_component::{Theme, ThemeMode, ThemeRegistry, scroll::ScrollbarShow};
use serde::{Deserialize, Serialize};

use crate::storage::get_config_dir;

const THEME_STATE_FILE: &str = "theme_state.json";
const LEGACY_STATE_FILE: &str = "target/state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThemeState {
    theme: SharedString,
    scrollbar_show: Option<ScrollbarShow>,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self {
            theme: "Default Light".into(),
            scrollbar_show: Some(ScrollbarShow::Hover),
        }
    }
}

fn get_theme_state_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }
    Ok(config_dir.join(THEME_STATE_FILE))
}

pub fn init(cx: &mut App) {
    // Migrate from legacy path if needed
    if std::path::Path::new(LEGACY_STATE_FILE).exists() {
        if let Ok(legacy_json) = std::fs::read_to_string(LEGACY_STATE_FILE) {
            if let Ok(legacy_state) = serde_json::from_str::<ThemeState>(&legacy_json) {
                if let Ok(new_path) = get_theme_state_path() {
                    if let Ok(json) = serde_json::to_string_pretty(&legacy_state) {
                        let _ = std::fs::write(&new_path, json);
                        let _ = std::fs::remove_file(LEGACY_STATE_FILE);
                        tracing::info!("Migrated theme state from legacy path to {:?}", new_path);
                    }
                }
            }
        }
    }

    // Load last theme state from new path
    tracing::info!("Load themes...");
    let state = match get_theme_state_path() {
        Ok(path) => {
            let json = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str::<ThemeState>(&json).unwrap_or_default()
        }
        Err(e) => {
            tracing::warn!("Failed to get theme state path: {}", e);
            ThemeState::default()
        }
    };

    if let Some(scrollbar_show) = state.scrollbar_show {
        Theme::global_mut(cx).scrollbar_show = scrollbar_show;
    }
    cx.refresh_windows();

    cx.on_action(|switch: &SwitchTheme, cx| {
        let theme_name = switch.0.clone();
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
        }
        cx.refresh_windows();
    });
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);
