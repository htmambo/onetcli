use crate::{Colorize, ThemeColor, ThemeMode, highlighter::HighlightThemeStyle};
use gpui::Hsla;

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
        0.96,
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

fn frosted_surface_tuning(mode: ThemeMode, opacity: f32) -> GlassSurfaceTuning {
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
