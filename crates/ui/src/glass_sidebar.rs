//! 侧边栏毛玻璃效果辅助函数
//!
//! 提供统一的侧边栏毛玻璃透明度调整逻辑，
//! 替代各模块中重复的本地实现。

use gpui::Hsla;

/// 返回侧边栏背景色，保持其 alpha 不变。
///
/// 该函数存在的意义是统一侧边栏背景色的获取入口，
/// 确保调用者不会意外覆盖掉 apply_glass_tuning 已经算好的 frosted alpha。
///
/// # Arguments
/// * `color` - 已经过 apply_glass_tuning 处理的 sidebar 颜色（alpha 已正确）
/// * `_blur_enabled` - 已废弃，忽略
/// * `_window_opacity` - 已废弃，忽略
///
/// # Example
/// ```ignore
/// div().bg(glass_sidebar(cx.theme().sidebar, blur_enabled, cx.theme().window_opacity))
/// ```
pub fn glass_sidebar(color: Hsla, _blur_enabled: bool, _window_opacity: f32) -> Hsla {
    color
}
