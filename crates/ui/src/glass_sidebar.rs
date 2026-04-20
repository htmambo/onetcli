//! 侧边栏毛玻璃效果辅助函数
//!
//! 提供统一的侧边栏毛玻璃透明度调整逻辑，
//! 替代各模块中重复的本地实现。

use crate::theme::LEFT_PANEL_ALPHA_OFFSET;
use gpui::Hsla;

/// 为侧边栏背景应用毛玻璃透明度
///
/// blur_enabled 仅控制 frosted 视觉效果（是否在颜色上应用 frosted tuning），
/// 实际的 alpha 透明度始终由 window_opacity 控制。
///
/// 当 blur_enabled=true 时，在 window_opacity 基础上增加 LEFT_PANEL_ALPHA_OFFSET，
/// 以增强侧边栏的毛玻璃质感。
///
/// 当 blur_enabled=false 时，使用 window_opacity 作为 alpha，
/// 不额外增加偏移量。
///
/// # Arguments
/// * `color` - 原始背景色（已经过 apply_glass_tuning 处理）
/// * `blur_enabled` - 是否启用 frosted 效果
/// * `window_opacity` - 窗口透明度（0.0~1.0）
///
/// # Example
/// ```ignore
/// div().bg(glass_sidebar(cx.theme().sidebar, blur_enabled, cx.theme().window_opacity))
/// ```
pub fn glass_sidebar(color: Hsla, blur_enabled: bool, window_opacity: f32) -> Hsla {
    let alpha = if blur_enabled {
        (window_opacity + LEFT_PANEL_ALPHA_OFFSET).clamp(0.0, 1.0)
    } else {
        window_opacity.clamp(0.0, 1.0)
    };
    let mut c = color;
    c.a = alpha;
    c
}

/// 为侧边栏背景应用毛玻璃透明度（接受 f64 参数）
pub fn glass_sidebar_f64(color: Hsla, blur_enabled: bool, window_opacity: f64) -> Hsla {
    glass_sidebar(color, blur_enabled, window_opacity as f32)
}
