//! 字体设置辅助（S8 抽取）
//!
//! 原 setting_tab.rs 中字体相关的 free-standing 函数（枚举字体选项、加载自
//! 定义字体、导入路径持久化）抽到本文件。`setting_tab.rs` 通过
//! `#[path = "_fonts.rs"] mod fonts;` 注册，避免与同名 setting_tab.rs 冲突。

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gpui::{App, SharedString};
use rust_i18n::t;

use crate::setting_tab::app_settings::AppSettings;

/// 内置等宽字体选项
pub(super) fn monospace_font_options() -> Vec<(SharedString, SharedString)> {
    [
        "Menlo",
        "Consolas",
        "JetBrains Mono",
        "Fira Code",
        "Cascadia Mono",
        "DejaVu Sans Mono",
        "Source Code Pro",
        "Noto Sans Mono CJK SC",
        "Source Han Mono SC",
        "Microsoft YaHei",
        "PingFang SC",
        "Courier New",
    ]
    .into_iter()
    .map(|font| (font.into(), font.into()))
    .collect()
}

/// 在内置选项后追加用户已配置的自定义字体路径
pub(super) fn mono_font_options_with_custom(custom_paths: &[String]) -> Vec<(SharedString, SharedString)> {
    let mut options = monospace_font_options();
    for path in custom_paths {
        // 必须拥有字符串，避免 SharedString 从临时 &str 泄漏借用。
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        let label: SharedString = name.into();
        if !options.iter().any(|(value, _)| value == &label) {
            options.push((label.clone(), label));
        }
    }
    options
}

const FONT_FILE_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

pub(super) fn is_supported_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            FONT_FILE_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

pub(super) fn load_custom_font_path(path: &Path, cx: &mut App) -> Result<(), String> {
    if !is_supported_font_file(path) {
        return Err(t!("Settings.General.Font.unsupported_font_file").to_string());
    }
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    cx.text_system()
        .add_fonts(vec![Cow::Owned(bytes)])
        .map_err(|err| err.to_string())
}

pub(super) fn load_custom_fonts(paths: &[String], cx: &mut App) -> usize {
    paths
        .iter()
        .filter(|path| load_custom_font_path(Path::new(path), cx).is_ok())
        .count()
}

pub(super) fn import_custom_font_paths(paths: Vec<PathBuf>, cx: &mut App) -> String {
    // 先加载字体，再写入设置，避免 AppSettings 可变借用与 text_system 冲突。
    let mut loaded_paths = Vec::new();
    for path in paths {
        if load_custom_font_path(&path, cx).is_err() {
            continue;
        }
        loaded_paths.push(path.to_string_lossy().to_string());
    }

    if loaded_paths.is_empty() {
        return t!("Settings.General.Font.custom_fonts_import_empty").to_string();
    }

    let loaded = loaded_paths.len();
    {
        let settings = AppSettings::global_mut(cx);
        for path in loaded_paths {
            if !settings
                .custom_font_paths
                .iter()
                .any(|existing| existing == &path)
            {
                settings.custom_font_paths.push(path);
            }
        }
        settings.save();
    }
    let settings = AppSettings::global(cx).clone();
    settings.sync_db_view_settings(cx);
    t!(
        "Settings.General.Font.custom_fonts_import_success",
        count = loaded
    )
    .to_string()
}