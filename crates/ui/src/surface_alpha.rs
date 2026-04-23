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
    // backdrop_opacity 控制实际的透明度，blur_enabled 仅控制 frosted 视觉效果。
    // 关闭毛玻璃时，不再叠加 offset，避免子层级反而更不透明。
    let alpha = if blur_enabled {
        backdrop_opacity + offset
    } else {
        backdrop_opacity
    };
    with_alpha(color, alpha)
}

pub fn level_surface_color(
    color: Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
    level_ratio: f32,
) -> Hsla {
    // backdrop_opacity 控制实际的透明度，blur_enabled 仅控制 frosted 视觉效果。
    // 毛玻璃开启时，更高层级需要更不透明以增强 frosted 可读性（加法偏移）。
    // 关闭毛玻璃时，层级差异应通过颜色本身区分，不再增加 alpha，
    // 否则子层级反而更不透明，桌面透出效果变差。
    let alpha = if blur_enabled {
        backdrop_opacity + (level_ratio - LEVEL_RATIO_BASELINE)
    } else {
        backdrop_opacity
    };
    with_alpha(color, alpha)
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
    fn offset_surface_color_applies_window_opacity_without_blur() {
        let color = Hsla {
            h: 0.42,
            s: 0.31,
            l: 0.27,
            a: 0.13,
        };

        // 关闭 blur 时仅应用 backdrop_opacity，不叠加 offset
        let result = offset_surface_color(color, false, 0.84, 0.02);

        assert_eq!(result.h, color.h);
        assert_eq!(result.s, color.s);
        assert_eq!(result.l, color.l);
        assert_alpha_eq(result.a, 0.84);
    }

    #[test]
    fn offset_surface_color_applies_offset_when_blur_enabled() {
        let color = Hsla {
            h: 0.42,
            s: 0.31,
            l: 0.27,
            a: 0.13,
        };

        // 开启 blur 时叠加 offset
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
    fn level_surface_color_ignores_level_ratio_when_blur_disabled() {
        let color = Hsla {
            h: 0.13,
            s: 0.22,
            l: 0.74,
            a: 0.09,
        };

        // 关闭 blur 时忽略 level_ratio，仅使用 backdrop_opacity
        let result = level_surface_color(color, false, 0.84, 0.10);

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
