use gpui::{App, Global, SharedString};
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
    /// SQL 查询最大返回行数；0 表示不限制
    pub sql_query_max_rows: u32,
}

impl Default for DbViewSettings {
    fn default() -> Self {
        Self {
            db_undo_stack_size: DEFAULT_DB_UNDO_STACK_SIZE,
            large_text_editor_open_mode: LargeTextEditorOpenMode::default(),
            sql_query_max_rows: 1000,
        }
    }
}

impl Global for DbViewSettings {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbViewFontSettings {
    pub sql_editor_font_family: SharedString,
    pub table_preview_font_family: SharedString,
}

impl Default for DbViewFontSettings {
    fn default() -> Self {
        Self {
            sql_editor_font_family: default_db_mono_font(),
            table_preview_font_family: default_db_mono_font(),
        }
    }
}

impl Global for DbViewFontSettings {}

fn default_db_mono_font() -> SharedString {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    }
    .into()
}

pub fn init_settings(cx: &mut App, settings: DbViewSettings) {
    if cx.has_global::<DbViewSettings>() {
        *cx.global_mut::<DbViewSettings>() = settings;
    } else {
        cx.set_global(settings);
    }
}

pub fn init_font_settings(cx: &mut App, fonts: DbViewFontSettings) {
    if cx.has_global::<DbViewFontSettings>() {
        *cx.global_mut::<DbViewFontSettings>() = fonts;
    } else {
        cx.set_global(fonts);
    }
}

pub fn current_settings(cx: &App) -> DbViewSettings {
    cx.try_global::<DbViewSettings>()
        .copied()
        .unwrap_or_default()
}

pub fn current_font_settings(cx: &App) -> DbViewFontSettings {
    cx.try_global::<DbViewFontSettings>()
        .cloned()
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

pub fn current_sql_query_max_rows(cx: &App) -> Option<usize> {
    let max_rows = current_settings(cx).sql_query_max_rows;
    (max_rows > 0).then_some(max_rows as usize)
}

pub fn current_sql_editor_font_family(cx: &App) -> SharedString {
    current_font_settings(cx).sql_editor_font_family
}

pub fn current_table_preview_font_family(cx: &App) -> SharedString {
    current_font_settings(cx).table_preview_font_family
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

    #[test]
    fn db_view_settings_default_sql_query_max_rows() {
        assert_eq!(DbViewSettings::default().sql_query_max_rows, 1000);
    }
}
