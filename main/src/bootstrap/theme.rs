use gpui::App;

use crate::setting_tab::AppSettings;

/// 初始化主题目录复制与监听。
pub fn init_theme_runtime(cx: &mut App) {
    if let Err(err) = one_core::storage::ensure_themes_copied() {
        tracing::warn!("Failed to copy bundled themes: {}", err);
    }

    let themes_dir = one_core::storage::get_runtime_themes_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("./themes"));
    if let Err(err) = gpui_component::ThemeRegistry::watch_dir(themes_dir, cx, |cx| {
        let settings = AppSettings::global(cx).clone();
        cx.defer(move |cx| {
            settings.apply_theme_preferences(None, cx);
        });
    }) {
        tracing::error!("Failed to watch themes directory: {}", err);
    }
}
