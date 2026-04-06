use crate::{Colorize, Theme, ThemeColor, ThemeMode, highlighter::HighlightThemeStyle};
use gpui::Hsla;

const DIALOG_SURFACE_BASE_OPACITY: f32 = 0.50;
const DIALOG_CHROME_ALPHA_OFFSET: f32 = 0.20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogSurfaceRole {
    Content,
    Chrome,
}

#[derive(Clone, Copy, Debug)]
pub struct ModalSurfacePalette {
    pub content: Hsla,
    pub title_bar: Hsla,
    pub footer: Hsla,
}

#[derive(Clone, Copy)]
struct GlassSurfaceTuning {
    base: f32,
    elevated: f32,
    chrome: f32,
    hover: f32,
    active: f32,
    border: f32,
    divider: f32,
}

/// Keep primary data surfaces translucent so the window backdrop can show through.
pub(crate) fn apply_glass_tuning(
    colors: &mut ThemeColor,
    mode: ThemeMode,
    blur_enabled: bool,
    opacity: f32,
) {
    let tuning = if blur_enabled {
        frosted_surface_tuning(mode, opacity)
    } else {
        plain_surface_tuning(mode, opacity)
    };

    colors.background = surface_color(colors.background, mode, blur_enabled, tuning.base, 1.0);
    colors.border = with_alpha(colors.border, tuning.border);
    colors.group_box = surface_color(colors.group_box, mode, blur_enabled, tuning.elevated, 0.9);
    colors.input = with_alpha(colors.input, tuning.border);
    colors.list = surface_color(colors.list, mode, blur_enabled, tuning.base, 0.88);
    colors.list_even = surface_color(colors.list_even, mode, blur_enabled, tuning.base, 0.84);
    colors.list_head = surface_color(colors.list_head, mode, blur_enabled, tuning.elevated, 1.0);
    colors.list_hover = surface_color(colors.list_hover, mode, blur_enabled, tuning.hover, 0.72);
    colors.muted = surface_color(colors.muted, mode, blur_enabled, tuning.elevated, 0.82);
    colors.popover = surface_color(colors.popover, mode, blur_enabled, tuning.active, 1.0);
    colors.secondary = surface_color(colors.secondary, mode, blur_enabled, tuning.elevated, 0.86);
    colors.secondary_active = surface_color(
        colors.secondary_active,
        mode,
        blur_enabled,
        tuning.active,
        0.74,
    );
    colors.secondary_hover = surface_color(
        colors.secondary_hover,
        mode,
        blur_enabled,
        tuning.hover,
        0.72,
    );
    colors.sidebar = surface_color(colors.sidebar, mode, blur_enabled, tuning.chrome, 1.0);
    colors.sidebar_border = with_alpha(colors.sidebar_border, tuning.divider);
    colors.tab = surface_color(colors.tab, mode, blur_enabled, tuning.chrome, 0.92);
    colors.tab_active = surface_color(colors.tab_active, mode, blur_enabled, tuning.elevated, 1.0);
    colors.tab_bar = surface_color(colors.tab_bar, mode, blur_enabled, tuning.chrome, 1.0);
    colors.tab_bar_segmented = surface_color(
        colors.tab_bar_segmented,
        mode,
        blur_enabled,
        tuning.chrome,
        0.6,
    );
    colors.table = surface_color(colors.table, mode, blur_enabled, tuning.base, 0.88);
    colors.table_even = surface_color(colors.table_even, mode, blur_enabled, tuning.base, 0.84);
    colors.table_head = surface_color(colors.table_head, mode, blur_enabled, tuning.elevated, 1.0);
    colors.table_hover = surface_color(colors.table_hover, mode, blur_enabled, tuning.hover, 0.72);
    colors.table_row_border = with_alpha(colors.table_row_border, tuning.divider);
    colors.title_bar = surface_color(colors.title_bar, mode, blur_enabled, tuning.chrome, 1.0);
    colors.title_bar_border = with_alpha(colors.title_bar_border, tuning.border);
    colors.tiles = surface_color(colors.tiles, mode, blur_enabled, tuning.chrome, 0.94);
    colors.window_border = with_alpha(colors.window_border, tuning.divider);
}

pub(crate) fn apply_glass_highlight_tuning(
    style: &mut HighlightThemeStyle,
    blur_enabled: bool,
    opacity: f32,
) {
    let editor_alpha = if blur_enabled {
        offset_alpha(opacity, 0.01)
    } else {
        opacity
    };
    let active_line_alpha = if blur_enabled {
        offset_alpha(opacity, 0.08)
    } else {
        offset_alpha(opacity, 0.04)
    };

    if let Some(background) = style.editor_background {
        style.editor_background = Some(with_alpha(background, editor_alpha));
    }

    if let Some(active_line) = style.editor_active_line {
        style.editor_active_line = Some(with_alpha(active_line, active_line_alpha));
    }
}

pub(crate) fn dialog_content_surface_color(color: Hsla, blur_enabled: bool, opacity: f32) -> Hsla {
    with_alpha(
        color,
        dialog_surface_alpha(blur_enabled, opacity, DialogSurfaceRole::Content),
    )
}

pub(crate) fn dialog_chrome_surface_color(color: Hsla, blur_enabled: bool, opacity: f32) -> Hsla {
    with_alpha(
        color,
        dialog_surface_alpha(blur_enabled, opacity, DialogSurfaceRole::Chrome),
    )
}

pub fn modal_surface_palette(theme: &Theme) -> ModalSurfacePalette {
    let colors = theme.colors_without_glass();
    let blur_enabled = theme.window_blur_enabled;
    let opacity = theme.surface_opacity;

    ModalSurfacePalette {
        content: dialog_content_surface_color(colors.background, blur_enabled, opacity),
        title_bar: dialog_chrome_surface_color(colors.title_bar, blur_enabled, opacity),
        footer: dialog_chrome_surface_color(colors.secondary, blur_enabled, opacity),
    }
}

fn frosted_surface_tuning(mode: ThemeMode, opacity: f32) -> GlassSurfaceTuning {
    if cfg!(target_os = "macos") {
        return macos_frosted_surface_tuning(mode, opacity);
    }

    if mode.is_dark() {
        GlassSurfaceTuning {
            base: opacity,
            elevated: offset_alpha(opacity, 0.06),
            chrome: offset_alpha(opacity, -0.04),
            hover: offset_alpha(opacity, 0.10),
            active: offset_alpha(opacity, 0.13),
            border: offset_alpha(opacity, -0.20),
            divider: offset_alpha(opacity, -0.36),
        }
    } else {
        GlassSurfaceTuning {
            base: opacity,
            elevated: offset_alpha(opacity, 0.06),
            chrome: offset_alpha(opacity, -0.04),
            hover: offset_alpha(opacity, 0.11),
            active: offset_alpha(opacity, 0.13),
            border: offset_alpha(opacity, -0.14),
            divider: offset_alpha(opacity, -0.32),
        }
    }
}

fn macos_frosted_surface_tuning(mode: ThemeMode, opacity: f32) -> GlassSurfaceTuning {
    if mode.is_dark() {
        GlassSurfaceTuning {
            base: offset_alpha(opacity, -0.12),
            elevated: offset_alpha(opacity, -0.08),
            chrome: offset_alpha(opacity, -0.22),
            hover: offset_alpha(opacity, -0.06),
            active: offset_alpha(opacity, -0.02),
            border: offset_alpha(opacity, -0.28),
            divider: offset_alpha(opacity, -0.38),
        }
    } else {
        GlassSurfaceTuning {
            base: offset_alpha(opacity, -0.14),
            elevated: offset_alpha(opacity, -0.09),
            chrome: offset_alpha(opacity, -0.24),
            hover: offset_alpha(opacity, -0.05),
            active: offset_alpha(opacity, -0.01),
            border: offset_alpha(opacity, -0.26),
            divider: offset_alpha(opacity, -0.34),
        }
    }
}

fn plain_surface_tuning(mode: ThemeMode, opacity: f32) -> GlassSurfaceTuning {
    if mode.is_dark() {
        GlassSurfaceTuning {
            base: opacity,
            elevated: offset_alpha(opacity, 0.02),
            chrome: offset_alpha(opacity, 0.01),
            hover: offset_alpha(opacity, 0.05),
            active: offset_alpha(opacity, 0.07),
            border: offset_alpha(opacity, -0.08),
            divider: offset_alpha(opacity, -0.16),
        }
    } else {
        GlassSurfaceTuning {
            base: opacity,
            elevated: offset_alpha(opacity, 0.02),
            chrome: offset_alpha(opacity, 0.01),
            hover: offset_alpha(opacity, 0.05),
            active: offset_alpha(opacity, 0.07),
            border: offset_alpha(opacity, -0.08),
            divider: offset_alpha(opacity, -0.14),
        }
    }
}

fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

fn surface_color(
    color: Hsla,
    mode: ThemeMode,
    blur_enabled: bool,
    alpha: f32,
    frost_intensity: f32,
) -> Hsla {
    let color = if blur_enabled {
        frost_color(color, mode, frost_intensity)
    } else {
        color
    };

    with_alpha(color, alpha)
}

fn frost_color(color: Hsla, mode: ThemeMode, intensity: f32) -> Hsla {
    let intensity = intensity.clamp(0.0, 1.0);
    if mode.is_dark() {
        color
            .saturation((color.s * (1.0 - 0.55 * intensity)).clamp(0.0, 1.0))
            .lightness((color.l + 0.12 * intensity).clamp(0.0, 1.0))
    } else {
        color
            .saturation((color.s * (1.0 - 0.35 * intensity)).clamp(0.0, 1.0))
            .lightness((color.l + 0.06 * intensity).clamp(0.0, 1.0))
    }
}

fn offset_alpha(alpha: f32, delta: f32) -> f32 {
    (alpha + delta).clamp(0.0, 1.0)
}

fn dialog_surface_alpha(blur_enabled: bool, opacity: f32, role: DialogSurfaceRole) -> f32 {
    let opacity = opacity.clamp(0.0, 1.0);
    if !blur_enabled && opacity >= 1.0 {
        return 1.0;
    }

    let base_alpha = opacity.max(DIALOG_SURFACE_BASE_OPACITY);
    match role {
        DialogSurfaceRole::Content => base_alpha,
        DialogSurfaceRole::Chrome => offset_alpha(base_alpha, DIALOG_CHROME_ALPHA_OFFSET),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::apply_glass_tuning;

    fn assert_alpha_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "expected alpha {expected}, got {actual}"
        );
    }

    #[test]
    fn dialog_content_surface_alpha_uses_minimum_when_translucent() {
        assert_alpha_eq(
            dialog_surface_alpha(false, 0.40, DialogSurfaceRole::Content),
            0.80,
        );
        assert_alpha_eq(
            dialog_surface_alpha(true, 0.40, DialogSurfaceRole::Content),
            0.80,
        );
    }

    #[test]
    fn dialog_surface_alpha_uses_app_opacity_when_above_minimum() {
        assert_alpha_eq(
            dialog_surface_alpha(true, 0.84, DialogSurfaceRole::Content),
            0.84,
        );
        assert_alpha_eq(
            dialog_surface_alpha(false, 0.84, DialogSurfaceRole::Chrome),
            0.94,
        );
    }

    #[test]
    fn dialog_surface_alpha_keeps_opaque_dialogs_fully_opaque() {
        assert_alpha_eq(
            dialog_surface_alpha(false, 1.0, DialogSurfaceRole::Content),
            1.0,
        );
        assert_alpha_eq(
            dialog_surface_alpha(false, 1.0, DialogSurfaceRole::Chrome),
            1.0,
        );
    }

    #[test]
    fn dialog_chrome_surface_alpha_clamps_to_one() {
        assert_alpha_eq(
            dialog_surface_alpha(true, 0.96, DialogSurfaceRole::Chrome),
            1.0,
        );
    }

    #[test]
    fn dialog_surface_color_only_overrides_alpha() {
        let color = Hsla {
            h: 0.23,
            s: 0.42,
            l: 0.51,
            a: 0.17,
        };

        let dialog_color = dialog_content_surface_color(color, true, 0.84);

        assert_eq!(dialog_color.h, color.h);
        assert_eq!(dialog_color.s, color.s);
        assert_eq!(dialog_color.l, color.l);
        assert_alpha_eq(
            dialog_color.a,
            dialog_surface_alpha(true, 0.84, DialogSurfaceRole::Content),
        );
    }

    #[test]
    fn dialog_chrome_surface_color_only_overrides_alpha() {
        let color = Hsla {
            h: 0.61,
            s: 0.18,
            l: 0.43,
            a: 0.29,
        };

        let dialog_color = dialog_chrome_surface_color(color, true, 0.84);

        assert_eq!(dialog_color.h, color.h);
        assert_eq!(dialog_color.s, color.s);
        assert_eq!(dialog_color.l, color.l);
        assert_alpha_eq(
            dialog_color.a,
            dialog_surface_alpha(true, 0.84, DialogSurfaceRole::Chrome),
        );
    }

    #[test]
    fn modal_surface_palette_uses_non_glass_base_colors() {
        let mut theme = Theme::from(ThemeColor::light().as_ref());
        theme.mode = ThemeMode::Light;
        theme.window_blur_enabled = true;
        theme.surface_opacity = 0.84;
        apply_glass_tuning(
            &mut theme.colors,
            theme.mode,
            theme.window_blur_enabled,
            theme.surface_opacity,
        );

        let raw_colors = theme.colors_without_glass();
        let palette = modal_surface_palette(&theme);

        assert_eq!(palette.content.h, raw_colors.background.h);
        assert_eq!(palette.content.s, raw_colors.background.s);
        assert_eq!(palette.content.l, raw_colors.background.l);
        assert_alpha_eq(palette.content.a, 0.84);

        assert_eq!(palette.title_bar.h, raw_colors.title_bar.h);
        assert_eq!(palette.title_bar.s, raw_colors.title_bar.s);
        assert_eq!(palette.title_bar.l, raw_colors.title_bar.l);
        assert_alpha_eq(palette.title_bar.a, 0.94);

        assert_eq!(palette.footer.h, raw_colors.secondary.h);
        assert_eq!(palette.footer.s, raw_colors.secondary.s);
        assert_eq!(palette.footer.l, raw_colors.secondary.l);
        assert_alpha_eq(palette.footer.a, 0.94);
    }
}
