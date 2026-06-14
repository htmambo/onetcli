//! 系统全局唤起热键的默认值与 serde 默认值函数。
//!
//! 抽取自 `setting_tab.rs`（轮 1 重构）。常量保持 `pub(crate)` 以维持
//! `crate::setting_tab::DEFAULT_SYSTEM_HOTKEY_*` 外部引用路径不变；
//! 默认值函数仅供父模块的 `#[serde(default = "...")]` 与
//! `impl Default for AppSettings` 使用。

pub(crate) const DEFAULT_SYSTEM_HOTKEY_MACOS: &str = "cmd-alt-m";
pub(crate) const DEFAULT_SYSTEM_HOTKEY_OTHER: &str = "ctrl-alt-m";

pub(super) fn default_system_hotkey_macos() -> String {
    DEFAULT_SYSTEM_HOTKEY_MACOS.to_string()
}

pub(super) fn default_system_hotkey_other() -> String {
    DEFAULT_SYSTEM_HOTKEY_OTHER.to_string()
}
