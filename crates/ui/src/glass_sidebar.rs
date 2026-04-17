//! 侧边栏毛玻璃效果辅助函数
//!
//! 提供统一的侧边栏毛玻璃透明度调整逻辑，
//! 替代各模块中重复的本地实现。

use crate::theme::LEFT_PANEL_ALPHA_OFFSET;
use gpui::Hsla;

/// 为侧边栏背景应用毛玻璃透明度
///
/// 在 blur 启用时，将 opacity + 偏移量作为 alpha 值，
/// 否则返回原始颜色。
///
/// # Arguments
/// * `color` - 原始背景色
/// * `blur_enabled` - 是否启用 blur
/// * `opacity` - 基础透明度（0.0~1.0）
///
/// # Example
/// ```ignore
/// div().bg(glass_sidebar(cx.theme().sidebar, blur_enabled, 0.84))
/// ```
pub fn glass_sidebar(color: Hsla, blur_enabled: bool, opacity: f32) -> Hsla {
    if blur_enabled {
        let mut c = color;
        c.a = (opacity + LEFT_PANEL_ALPHA_OFFSET).clamp(0.0, 1.0);
        c
    } else {
        color
    }
}

/// 为侧边栏背景应用毛玻璃透明度（接受 f64 参数）
pub fn glass_sidebar_f64(color: Hsla, blur_enabled: bool, opacity: f64) -> Hsla {
    glass_sidebar(color, blur_enabled, opacity as f32)
}
