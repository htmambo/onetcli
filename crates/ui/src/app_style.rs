use gpui::{App, Hsla, StyleRefinement, Styled, rgb, rgba, white};

use crate::button::{ButtonCustomVariant, ButtonVariant};

pub fn page_bg() -> Hsla {
    rgb(0x0a0a0a).into()
}

pub fn panel_bg() -> Hsla {
    rgb(0x12121a).into()
}

pub fn panel_alt_bg() -> Hsla {
    rgb(0x101114).into()
}

pub fn panel_hover_bg() -> Hsla {
    rgb(0x15171d).into()
}

pub fn border() -> Hsla {
    rgba(0xffffff14).into()
}

pub fn border_strong() -> Hsla {
    rgba(0xffffff1f).into()
}

pub fn text_primary() -> Hsla {
    rgb(0xeaeaf0).into()
}

pub fn text_muted() -> Hsla {
    rgb(0xb7b7c6).into()
}

pub fn text_soft() -> Hsla {
    rgb(0x6b7280).into()
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
    rgb(0xef4444).into()
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
        .bg(panel_bg())
        .border_color(border())
        .text_color(text_primary())
}

pub fn page_header_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_alt_bg())
        .border_color(border())
        .text_color(text_primary())
}

pub fn title_bar_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_bg())
        .border_color(border())
        .text_color(text_primary())
}

pub fn footer_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(panel_bg())
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
            .foreground(danger())
            .border(danger().opacity(0.35))
            .hover(danger_dim())
            .active(danger().opacity(0.18)),
    )
}
