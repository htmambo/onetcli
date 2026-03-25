use std::sync::{LazyLock, RwLock};

use gpui::{App, Hsla, StyleRefinement, Styled, rgb, white};

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

pub fn page_bg() -> Hsla {
    active_theme().background
}

pub fn panel_bg() -> Hsla {
    active_theme().group_box
}

pub fn panel_alt_bg() -> Hsla {
    active_theme().secondary
}

pub fn panel_hover_bg() -> Hsla {
    active_theme().secondary_hover
}

pub fn border() -> Hsla {
    active_theme().border
}

pub fn border_strong() -> Hsla {
    active_theme().input
}

pub fn text_primary() -> Hsla {
    active_theme().foreground
}

pub fn text_muted() -> Hsla {
    active_theme().muted_foreground
}

pub fn text_soft() -> Hsla {
    active_theme().muted_foreground.opacity(0.72)
}

pub fn accent() -> Hsla {
    rgb(0x00d9a3).into()
}

pub fn accent_hover() -> Hsla {
    rgb(0x10b981).into()
}

pub fn accent_active() -> Hsla {
    rgb(0x059669).into()
}

pub fn accent_dim() -> Hsla {
    accent().opacity(0.10)
}

pub fn accent_dim_strong() -> Hsla {
    accent().opacity(0.16)
}

pub fn danger() -> Hsla {
    active_theme().danger
}

pub fn danger_dim() -> Hsla {
    danger().opacity(0.12)
}

pub fn surface_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_bg())
        .border_1()
        .border_color(border())
        .text_color(text_primary())
}

pub fn control_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_alt_bg())
        .border_color(border_strong())
        .text_color(text_primary())
}

pub fn sidebar_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(active_theme().sidebar)
        .border_color(active_theme().sidebar_border)
        .text_color(active_theme().sidebar_foreground)
}

pub fn page_header_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_alt_bg())
        .border_color(border())
        .text_color(text_primary())
}

pub fn title_bar_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(active_theme().title_bar)
        .border_color(active_theme().title_bar_border)
        .text_color(text_primary())
}

pub fn footer_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_alt_bg())
        .border_color(border())
        .text_color(text_primary())
}

pub fn primary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(accent())
            .foreground(white())
            .border(accent().opacity(0.4))
            .hover(accent_hover())
            .active(accent_active())
            .shadow(true),
    )
}

pub fn secondary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(panel_alt_bg())
            .foreground(text_primary())
            .border(border_strong())
            .hover(panel_hover_bg())
            .active(panel_bg()),
    )
}

pub fn danger_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(panel_alt_bg())
            .foreground(cx.theme().danger)
            .border(cx.theme().danger.opacity(0.35))
            .hover(cx.theme().danger_hover)
            .active(cx.theme().danger_active),
    )
}
