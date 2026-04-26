use crate::theme::{WindowsSurfaceLayer, windows_surface_color, windows_surface_opacity};
use gpui::Hsla;

const LEVEL_RATIO_BASELINE: f32 = 0.08;
const TERMINAL_BLUR_OPACITY_OFFSET: f32 = 0.05;

fn resolve_level_ratio(level: u8) -> f32 {
    match level {
        1 => 0.08,
        2 => 0.10,
        3 => 0.14,
        4 => 0.16,
        5 => 0.18,
        _ => panic!("invalid surface level: {level}, expected 1..=5"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayScrimLevel {
    Blocking,
    Loading,
}

fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha.clamp(0.0, 1.0);
    color
}

impl OverlayScrimLevel {
    const fn alpha(self) -> f32 {
        match self {
            Self::Blocking => 0.70,
            Self::Loading => 0.25,
        }
    }
}

/// `level` 为层级编号，从底到顶由 1 开始：
/// - `1`：最底层（最轻/最透明），如根背景。
/// - `2`：中间层，如侧边栏、标签栏。
/// - `3`：较重层，如内容区、工具栏。
/// - `4`：卡片层。
/// - `5`：最顶层（最重），如卡片叠加层。
pub fn level_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    level: u8,
) -> Hsla {
    let level_ratio = resolve_level_ratio(level);
    // backdrop_opacity 控制实际的透明度，blur_enabled 仅控制 frosted 视觉效果。
    // 毛玻璃开启时，更高层级需要更不透明以增强 frosted 可读性（加法偏移）。
    // 关闭毛玻璃时，层级差异应通过颜色本身区分，不再增加 alpha，
    // 否则子层级反而更不透明，桌面透出效果变差。
    let alpha = if blur_enabled {
        backdrop_opacity
        //  + (level_ratio - 0.0) * LEVEL_RATIO_BASELINE
    } else {
        backdrop_opacity
    };
    with_alpha(color, alpha)
}

/// `level` 为层级编号，从底到顶由 1 开始：
/// - `1`：最底层（最轻/最透明），如根背景。
/// - `2`：中间层，如侧边栏、标签栏。
/// - `3`：较重层，如内容区、工具栏。
/// - `4`：卡片层。
/// - `5`：最顶层（最重），如卡片叠加层。
pub fn layered_level_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    level: u8,
    layer: WindowsSurfaceLayer,
) -> Hsla {
    windows_surface_color(
        level_surface_color(color, blur_enabled, backdrop_opacity, level),
        blur_enabled,
        backdrop_opacity,
        layer,
    )
}

pub fn layered_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    layer: WindowsSurfaceLayer,
) -> Hsla {
    windows_surface_color(color, blur_enabled, backdrop_opacity, layer)
}

/// Pass-through: the color's alpha is already set by `apply_glass_tuning` at theme init time.
/// Do not overwrite it again.
#[inline]

pub fn overlay_scrim_color(color: Hsla, level: OverlayScrimLevel) -> Hsla {
    with_alpha(color, level.alpha())
}

pub fn terminal_canvas_surface_opacity(backdrop_opacity: f32, blur_enabled: bool) -> f32 {
    if cfg!(target_os = "windows") {
        windows_surface_opacity(
            backdrop_opacity,
            blur_enabled,
            WindowsSurfaceLayer::TerminalCanvas,
        )
    } else if blur_enabled {
        (backdrop_opacity + TERMINAL_BLUR_OPACITY_OFFSET).clamp(0.0, 1.0)
    } else {
        backdrop_opacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_alpha_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "expected alpha {expected}, got {actual}"
        );
    }

    #[test]
    fn level_surface_color_uses_shared_level_baseline() {
        let color = Hsla {
            h: 0.13,
            s: 0.22,
            l: 0.74,
            a: 0.09,
        };

        let result = level_surface_color(color, true, 0.84, 2);

        assert_alpha_eq(result.a, 0.86);
    }

    #[test]
    fn level_surface_color_ignores_level_ratio_when_blur_disabled() {
        let color = Hsla {
            h: 0.13,
            s: 0.22,
            l: 0.74,
            a: 0.09,
        };

        // 关闭 blur 时忽略 level_ratio，仅使用 backdrop_opacity
        let result = level_surface_color(color, false, 0.84, 2);

        assert_alpha_eq(result.a, 0.84);
    }

    #[test]
    fn layered_level_surface_color_matches_platform_rules() {
        let color = Hsla {
            h: 0.66,
            s: 0.19,
            l: 0.53,
            a: 0.21,
        };

        let result = layered_level_surface_color(
            color,
            true,
            0.84,
            2,
            WindowsSurfaceLayer::ContentSection,
        );

        if cfg!(target_os = "windows") {
            assert_alpha_eq(result.a, 0.84 * 0.48);
        } else {
            assert_alpha_eq(result.a, 0.86);
        }
    }

    #[test]
    fn layered_surface_color_only_applies_windows_layering() {
        let color = Hsla {
            h: 0.66,
            s: 0.19,
            l: 0.53,
            a: 0.21,
        };

        let result = layered_surface_color(color, true, 0.84, WindowsSurfaceLayer::ContentSection);

        if cfg!(target_os = "windows") {
            assert_alpha_eq(result.a, 0.84 * 0.48);
        } else {
            assert_eq!(result, color);
        }
    }

    #[test]
    fn overlay_scrim_color_uses_blocking_strength() {
        let color = Hsla {
            h: 0.58,
            s: 0.14,
            l: 0.09,
            a: 0.01,
        };

        let result = overlay_scrim_color(color, OverlayScrimLevel::Blocking);

        assert_alpha_eq(result.a, 0.70);
    }

    #[test]
    fn overlay_scrim_color_uses_loading_strength() {
        let color = Hsla {
            h: 0.58,
            s: 0.14,
            l: 0.09,
            a: 0.99,
        };

        let result = overlay_scrim_color(color, OverlayScrimLevel::Loading);

        assert_alpha_eq(result.a, 0.25);
    }

    #[test]
    fn terminal_canvas_surface_opacity_uses_platform_specific_formula() {
        let opacity = 0.84;
        let result = terminal_canvas_surface_opacity(opacity, true);

        if cfg!(target_os = "windows") {
            assert_alpha_eq(result, opacity * 0.52);
        } else {
            assert_alpha_eq(result, 0.89);
        }
    }

    #[test]
    fn terminal_canvas_surface_opacity_keeps_plain_opacity_without_blur() {
        let opacity = 0.84;
        let result = terminal_canvas_surface_opacity(opacity, false);

        if cfg!(target_os = "windows") {
            assert_alpha_eq(result, opacity * 0.34);
        } else {
            assert_alpha_eq(result, opacity);
        }
    }
}
