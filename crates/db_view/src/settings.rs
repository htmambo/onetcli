use gpui::{App, Global};
use serde::{Deserialize, Serialize};

const DEFAULT_DB_UNDO_STACK_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LargeTextEditorOpenMode {
    #[default]
    SidebarPreview,
    Dialog,
}

impl LargeTextEditorOpenMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LargeTextEditorOpenMode::SidebarPreview => "sidebar_preview",
            LargeTextEditorOpenMode::Dialog => "dialog",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "dialog" => LargeTextEditorOpenMode::Dialog,
            _ => LargeTextEditorOpenMode::SidebarPreview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbViewSettings {
    pub db_undo_stack_size: usize,
    pub large_text_editor_open_mode: LargeTextEditorOpenMode,
}

impl Default for DbViewSettings {
    fn default() -> Self {
        Self {
            db_undo_stack_size: DEFAULT_DB_UNDO_STACK_SIZE,
            large_text_editor_open_mode: LargeTextEditorOpenMode::default(),
        }
    }
}

impl Global for DbViewSettings {}

pub fn init_settings(cx: &mut App, settings: DbViewSettings) {
    if cx.has_global::<DbViewSettings>() {
        *cx.global_mut::<DbViewSettings>() = settings;
    } else {
        cx.set_global(settings);
    }
}

pub fn current_settings(cx: &App) -> DbViewSettings {
    cx.try_global::<DbViewSettings>()
        .copied()
        .unwrap_or_default()
}

pub fn set_db_view_settings(cx: &mut App, db_undo_stack_size: usize) {
    let mut settings = current_settings(cx);
    settings.db_undo_stack_size = db_undo_stack_size;
    init_settings(cx, settings);
}

pub fn current_db_undo_stack_size(cx: &App) -> usize {
    current_settings(cx).db_undo_stack_size
}

pub fn set_large_text_editor_open_mode(mode: LargeTextEditorOpenMode, cx: &mut App) {
    let mut settings = current_settings(cx);
    settings.large_text_editor_open_mode = mode;
    init_settings(cx, settings);
}

#[cfg(test)]
mod tests {
    use super::{DbViewSettings, LargeTextEditorOpenMode};

    #[test]
    fn large_text_editor_open_mode_defaults_to_sidebar_preview() {
        assert_eq!(
            LargeTextEditorOpenMode::from_str("unknown"),
            LargeTextEditorOpenMode::SidebarPreview
        );
    }

    #[test]
    fn large_text_editor_open_mode_parses_dialog() {
        assert_eq!(
            LargeTextEditorOpenMode::from_str("dialog"),
            LargeTextEditorOpenMode::Dialog
        );
    }

    #[test]
    fn db_view_settings_default_uses_expected_undo_stack_size() {
        assert_eq!(DbViewSettings::default().db_undo_stack_size, 50);
    }
}
