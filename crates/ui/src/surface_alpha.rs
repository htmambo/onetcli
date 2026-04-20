use crate::glass_sidebar;
use crate::theme::{WindowsSurfaceLayer, windows_surface_color, windows_surface_opacity};
use gpui::Hsla;

const LEVEL_RATIO_BASELINE: f32 = 0.08;
const TERMINAL_BLUR_OPACITY_OFFSET: f32 = 0.05;

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

pub fn offset_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    offset: f32,
) -> Hsla {
    // backdrop_opacity 控制实际的透明度，blur_enabled 仅控制 frosted 视觉效果
    #[allow(unused_variables)]
    let _blur_enabled = blur_enabled;
    with_alpha(color, backdrop_opacity + offset)
}

pub fn level_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    level_ratio: f32,
) -> Hsla {
    // backdrop_opacity 控制实际的透明度，blur_enabled 仅控制 frosted 视觉效果
    #[allow(unused_variables)]
    let _ = blur_enabled;
    with_alpha(color, backdrop_opacity + (level_ratio - LEVEL_RATIO_BASELINE))
}

pub fn layered_level_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    level_ratio: f32,
    layer: WindowsSurfaceLayer,
) -> Hsla {
    windows_surface_color(
        level_surface_color(color, blur_enabled, backdrop_opacity, level_ratio),
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
pub fn sidebar_surface_color(color: Hsla) -> Hsla {
    color
}

pub fn sidebar_surface_color_with_offset(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    offset: f32,
) -> Hsla {
    // 将 backdrop_opacity + offset 作为最终 alpha 应用，与 sidebar_surface_color
    // 保持一致的逻辑，只是多了 offset 偏移。
    #[allow(unused_variables)]
    let _ = blur_enabled;
    with_alpha(color, (backdrop_opacity + offset).clamp(0.0, 1.0))
}

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
    fn offset_surface_color_always_applies_window_opacity() {
        let color = Hsla {
            h: 0.42,
            s: 0.31,
            l: 0.27,
            a: 0.13,
        };

        // window_opacity 控制透明度，无论 blur 是否启用
        let result = offset_surface_color(color, false, 0.84, 0.02);

        assert_eq!(result.h, color.h);
        assert_eq!(result.s, color.s);
        assert_eq!(result.l, color.l);
        assert_alpha_eq(result.a, 0.86);
    }

    #[test]
    fn offset_surface_color_applies_offset_when_blur_enabled() {
        let color = Hsla {
            h: 0.42,
            s: 0.31,
            l: 0.27,
            a: 0.13,
        };

        let result = offset_surface_color(color, true, 0.84, 0.02);

        assert_eq!(result.h, color.h);
        assert_eq!(result.s, color.s);
        assert_eq!(result.l, color.l);
        assert_alpha_eq(result.a, 0.86);
    }

    #[test]
    fn level_surface_color_uses_shared_level_baseline() {
        let color = Hsla {
            h: 0.13,
            s: 0.22,
            l: 0.74,
            a: 0.09,
        };

        let result = level_surface_color(color, true, 0.84, 0.10);

        assert_alpha_eq(result.a, 0.86);
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
            0.10,
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
    fn sidebar_surface_color_is_pass_through() {
        let color = Hsla {
            h: 0.11,
            s: 0.28,
            l: 0.62,
            a: 0.05,
        };

        // sidebar_surface_color 是直通函数，颜色 alpha 已在 apply_glass_tuning 时设置好
        let result = sidebar_surface_color(color);

        // 直通：输入输出完全一致
        assert_eq!(result.h, color.h);
        assert_eq!(result.s, color.s);
        assert_eq!(result.l, color.l);
        assert_alpha_eq(result.a, color.a);
    }

    #[test]
    fn sidebar_surface_color_with_offset_applies_negative_offset() {
        let color = Hsla {
            h: 0.11,
            s: 0.28,
            l: 0.62,
            a: 0.05,
        };

        let result = sidebar_surface_color_with_offset(color, true, 0.84, -0.10);

        // window_opacity(0.84) + offset(-0.10) = 0.74
        assert_alpha_eq(result.a, 0.74);
    }

    #[test]
    fn sidebar_surface_color_with_offset_applies_opacity_without_blur() {
        let color = Hsla {
            h: 0.11,
            s: 0.28,
            l: 0.62,
            a: 0.05,
        };

        // window_opacity 控制透明度，无论 blur 是否启用
        let result = sidebar_surface_color_with_offset(color, false, 0.84, -0.10);

        assert_eq!(result.h, color.h);
        assert_eq!(result.s, color.s);
        assert_eq!(result.l, color.l);
        // window_opacity(0.84) + offset(-0.10) = 0.74
        assert_alpha_eq(result.a, 0.74);
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
