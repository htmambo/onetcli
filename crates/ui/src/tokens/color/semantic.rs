//! 语义化颜色 token（Semantic Colors）
//!
//! 供组件使用。名称表达用途而非具体色值。

use super::modes::{ColorModes, DarkMode, LightMode};
use super::primitives::{dark, light};
use gpui::Hsla;

/// 语义化颜色结构体 — dark 模式
#[derive(Debug, Clone, Copy)]
pub struct SemanticColorsDark;

impl SemanticColorsDark {
    // === 背景层 ===
    pub const GLASS_BASE: Hsla = dark::BG_BASE;
    pub const GLASS_ELEVATED: Hsla = dark::BG_ELEVATED;
    pub const GLASS_CONTENT: Hsla = dark::BG_SURFACE;
    pub const GLASS_SURFACE: Hsla = dark::BG_SURFACE;
    pub const GLASS_SIDEBAR: Hsla = dark::BG_ELEVATED;

    // === 边框 ===
    pub const BORDER_SUBTLE: Hsla = dark::BORDER_SUBTLE;
    pub const BORDER_DEFAULT: Hsla = dark::BORDER_DEFAULT;
    pub const BORDER_STRONG: Hsla = dark::BORDER_DEFAULT;

    // === 文字 ===
    pub const TEXT_PRIMARY: Hsla = dark::TEXT_PRIMARY;
    pub const TEXT_SECONDARY: Hsla = dark::TEXT_SECONDARY;
    pub const TEXT_MUTED: Hsla = dark::TEXT_MUTED;
    pub const TEXT_LINK: Hsla = dark::PRIMARY;
    pub const TEXT_LINK_HOVER: Hsla = dark::ACCENT;

    // === 交互状态 ===
    pub const INTERACTIVE_PRIMARY: Hsla = dark::PRIMARY;
    pub const INTERACTIVE_HOVER: Hsla = dark::ACCENT;
    pub const INTERACTIVE_ACTIVE: Hsla = dark::ACCENT;
    pub const INTERACTIVE_DISABLED: Hsla = dark::TEXT_MUTED;

    // === 语义色 ===
    pub const SEMANTIC_SUCCESS: Hsla = dark::SUCCESS;
    pub const SEMANTIC_WARNING: Hsla = dark::WARNING;
    pub const SEMANTIC_DANGER: Hsla = dark::DANGER;
    pub const SEMANTIC_INFO: Hsla = dark::INFO;

    // === 特殊区域 ===
    pub const TITLE_BAR: Hsla = dark::BG_BASE;
    pub const TAB_BAR: Hsla = dark::BG_BASE;
    pub const TOOLBAR: Hsla = dark::BG_ELEVATED;
    pub const SCROLLBAR: Hsla = dark::BORDER_SUBTLE;
}

/// 语义化颜色结构体 — light 模式
#[derive(Debug, Clone, Copy)]
pub struct SemanticColorsLight;

impl SemanticColorsLight {
    pub const GLASS_BASE: Hsla = light::BG_BASE;
    pub const GLASS_ELEVATED: Hsla = light::BG_ELEVATED;
    pub const GLASS_CONTENT: Hsla = light::BG_SURFACE;
    pub const GLASS_SURFACE: Hsla = light::BG_SURFACE;
    pub const GLASS_SIDEBAR: Hsla = light::BG_ELEVATED;

    pub const BORDER_SUBTLE: Hsla = light::BORDER_SUBTLE;
    pub const BORDER_DEFAULT: Hsla = light::BORDER_DEFAULT;
    pub const BORDER_STRONG: Hsla = light::BORDER_DEFAULT;

    pub const TEXT_PRIMARY: Hsla = light::TEXT_PRIMARY;
    pub const TEXT_SECONDARY: Hsla = light::TEXT_SECONDARY;
    pub const TEXT_MUTED: Hsla = light::TEXT_MUTED;
    pub const TEXT_LINK: Hsla = light::PRIMARY;
    pub const TEXT_LINK_HOVER: Hsla = light::ACCENT;

    pub const INTERACTIVE_PRIMARY: Hsla = light::PRIMARY;
    pub const INTERACTIVE_HOVER: Hsla = light::ACCENT;
    pub const INTERACTIVE_ACTIVE: Hsla = light::ACCENT;
    pub const INTERACTIVE_DISABLED: Hsla = light::TEXT_MUTED;

    pub const SEMANTIC_SUCCESS: Hsla = light::SUCCESS;
    pub const SEMANTIC_WARNING: Hsla = light::WARNING;
    pub const SEMANTIC_DANGER: Hsla = light::DANGER;
    pub const SEMANTIC_INFO: Hsla = light::INFO;

    pub const TITLE_BAR: Hsla = light::BG_BASE;
    pub const TAB_BAR: Hsla = light::BG_BASE;
    pub const TOOLBAR: Hsla = light::BG_ELEVATED;
    pub const SCROLLBAR: Hsla = light::BORDER_SUBTLE;
}

/// 根据模式返回语义色的 ColorModes 实现
pub struct SemanticColors;

impl SemanticColors {
    pub fn get(mode: crate::theme::ThemeMode) -> Box<dyn ColorModes> {
        if mode.is_dark() {
            Box::new(DarkMode)
        } else {
            Box::new(LightMode)
        }
    }
}
