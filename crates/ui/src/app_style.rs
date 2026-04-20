//! 统一语义化样式工具函数。
//!
//! 所有 UI 区域（侧边栏、面板、工具栏、内容区等）的背景/边框/文字颜色统一
//! 引用本模块定义的一级语义 token，不再按区域命名（避免同一色值有多个名字）。
//!
//! 设计原则：
//! - 一级 token 描述"用途"而非"位置"（如 `surface` 而非 `sidebar_bg`）
//! - 二级组合函数（`_style()`）描述"组合方式"，供直接复用

use std::sync::{LazyLock, RwLock};

use gpui::{App, Hsla, StyleRefinement, Styled};

use crate::{
    Theme, ThemeColor,
    button::{ButtonCustomVariant, ButtonVariant},
    theme::ActiveTheme,
};

static ACTIVE_APP_STYLE_THEME: LazyLock<RwLock<ThemeColor>> =
    LazyLock::new(|| RwLock::new(*ThemeColor::light().as_ref()));

#[inline]
fn active_theme() -> ThemeColor {
    ACTIVE_APP_STYLE_THEME
        .read()
        .map(|theme| *theme)
        .unwrap_or_else(|_| *ThemeColor::light().as_ref())
}

pub(crate) fn sync_theme(theme: &Theme) {
    if let Ok(mut current_theme) = ACTIVE_APP_STYLE_THEME.write() {
        *current_theme = theme.colors;
    }
}

// =============================================================================
// 一级语义 token — 背景色
// =============================================================================

/// 页面/窗口最底层背景
pub fn base() -> Hsla {
    active_theme().background
}

/// 通用面板/卡片背景（设置页面板、弹窗内容区等）
pub fn surface() -> Hsla {
    active_theme().group
}

/// 替代/区分面板背景（页面头部、工具栏、分组背景）
pub fn surface_alt() -> Hsla {
    active_theme().secondary
}

/// 悬停态面板背景
pub fn surface_hover() -> Hsla {
    active_theme().secondary_hover
}

/// 侧边栏 / 工具栏背景
pub fn chrome() -> Hsla {
    active_theme().sidebar
}

/// 弹出层背景（Popover、Dropdown 等）
pub fn elevated() -> Hsla {
    active_theme().popover
}

// =============================================================================
// 一级语义 token — 边框色
// =============================================================================

/// 标准边框 / 分隔线
pub fn border() -> Hsla {
    active_theme().border
}

/// 强调边框（输入框等需要更醒目的边框）
pub fn border_strong() -> Hsla {
    active_theme().input
}

/// 侧边栏专属边框
pub fn chrome_border() -> Hsla {
    active_theme().sidebar_border
}

// =============================================================================
// 一级语义 token — 文字色
// =============================================================================

/// 主要文字
pub fn text() -> Hsla {
    active_theme().foreground
}

/// 次要/弱化文字
pub fn text_muted() -> Hsla {
    active_theme().muted_foreground
}

// =============================================================================
// 一级语义 token — 交互色
// =============================================================================

pub fn accent() -> Hsla {
    active_theme().success
}

pub fn accent_hover() -> Hsla {
    active_theme().success_hover
}

pub fn accent_active() -> Hsla {
    active_theme().success_active
}

pub fn accent_dim() -> Hsla {
    accent().opacity(0.10)
}

pub fn accent_dim_strong() -> Hsla {
    accent().opacity(0.16)
}

// =============================================================================
// 一级语义 token — 危险色
// =============================================================================

pub fn danger() -> Hsla {
    active_theme().danger
}

pub fn danger_dim() -> Hsla {
    danger().opacity(0.12)
}

// =============================================================================
// 二级组合样式
// =============================================================================

/// 通用面板样式：背景 + 边框 + 文字
pub fn surface_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(surface())
        .border_1()
        .border_color(border())
        .text_color(text())
}

/// 控件样式（输入框等）：alt 背景 + strong 边框
pub fn control_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(surface_alt())
        .border_color(border_strong())
        .text_color(text())
}

/// 侧边栏 / 工具栏统一样式：chrome 背景 + chrome 边框
pub fn chrome_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(chrome())
        .border_color(chrome_border())
        .text_color(text())
}

/// 页面头部样式：alt 背景 + 边框
pub fn page_header_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(surface_alt())
        .border_color(border())
        .text_color(text())
}

/// 标题栏样式
pub fn title_bar_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(active_theme().title_bar)
        .border_color(active_theme().title_bar_border)
        .text_color(text())
}

/// 底部栏样式
pub fn footer_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(surface_alt())
        .border_color(border())
        .text_color(text())
}

// =============================================================================
// 按钮样式
// =============================================================================

pub fn primary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(cx.theme().primary)
            .foreground(cx.theme().primary_foreground)
            .border(cx.theme().primary.opacity(0.4))
            .hover(cx.theme().primary_hover)
            .active(cx.theme().primary_active)
            .shadow(true),
    )
}

pub fn secondary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(surface_alt())
            .foreground(text())
            .border(border_strong())
            .hover(surface_hover())
            .active(surface()),
    )
}

pub fn danger_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(surface_alt())
            .foreground(cx.theme().danger)
            .border(cx.theme().danger.opacity(0.35))
            .hover(cx.theme().danger_hover)
            .active(cx.theme().danger_active),
    )
}

// =============================================================================
// 向后兼容别名（已有代码引用这些名称）
// =============================================================================

/// @deprecated use surface()
#[deprecated(since = "2.0.0", note = "use surface()")]
pub fn panel_bg() -> Hsla {
    surface()
}

/// @deprecated use surface_alt()
#[deprecated(since = "2.0.0", note = "use surface_alt()")]
pub fn panel_alt_bg() -> Hsla {
    surface_alt()
}

/// @deprecated use chrome()
#[deprecated(since = "2.0.0", note = "use chrome()")]
pub fn toolbar_bg() -> Hsla {
    chrome()
}

/// @deprecated use base()
#[deprecated(since = "2.0.0", note = "use base()")]
pub fn page_bg() -> Hsla {
    base()
}

/// @deprecated use chrome_style()
#[deprecated(since = "2.0.0", note = "use chrome_style()")]
pub fn sidebar_style() -> StyleRefinement {
    chrome_style()
}

/// @deprecated use text()
#[deprecated(since = "2.0.0", note = "use text()")]
pub fn text_primary() -> Hsla {
    text()
}

/// @deprecated use chrome()
#[deprecated(since = "2.0.0", note = "use chrome()")]
pub fn title_bar_bg() -> Hsla {
    // 注意：这里曾错误地指向 chrome()，导致标题栏和侧边栏背景色混用
    // 实际上 title_bar 有自己独立的 ThemeColor 字段
    active_theme().title_bar
}
