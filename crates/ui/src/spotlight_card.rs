use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Point, RenderOnce, StyleRefinement, Styled, Window, canvas, div, fill,
    outline, point, prelude::FluentBuilder as _, px, size,
};

use crate::ElementExt;

#[derive(IntoElement)]
pub struct SpotlightCard {
    id: ElementId,
    style: StyleRefinement,
    children: Vec<AnyElement>,
    accent: Hsla,
    glow: Hsla,
    hover_background: Option<Hsla>,
    radius: Pixels,
}

impl SpotlightCard {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            children: Vec::new(),
            accent: gpui::rgb(0x10b981).into(),
            glow: gpui::rgb(0x10b981).into(),
            hover_background: None,
            radius: px(16.0),
        }
    }

    pub fn accent(mut self, color: impl Into<Hsla>) -> Self {
        self.accent = color.into();
        self
    }

    pub fn glow(mut self, color: impl Into<Hsla>) -> Self {
        self.glow = color.into();
        self
    }

    pub fn hover_background(mut self, color: impl Into<Hsla>) -> Self {
        self.hover_background = Some(color.into());
        self
    }

    pub fn radius(mut self, radius: impl Into<Pixels>) -> Self {
        self.radius = radius.into();
        self
    }
}

impl Styled for SpotlightCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for SpotlightCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

#[derive(Clone, Copy, Default)]
struct SpotlightCardState {
    bounds: Bounds<Pixels>,
    hovered: bool,
    pointer: Point<Pixels>,
}

impl SpotlightCardState {
    fn set_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = bounds;
        if self.pointer == Point::default()
            && bounds.size.width > Pixels::ZERO
            && bounds.size.height > Pixels::ZERO
        {
            self.pointer = point(bounds.size.width / 2., bounds.size.height / 2.);
        }
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut App) {
        if self.hovered == hovered {
            return;
        }

        self.hovered = hovered;
        if hovered && self.pointer == Point::default() {
            self.pointer = point(self.bounds.size.width / 2., self.bounds.size.height / 2.);
        }
        cx.notify();
    }

    fn update_pointer(&mut self, window: &Window, cx: &mut App) {
        if self.bounds.size.width <= Pixels::ZERO || self.bounds.size.height <= Pixels::ZERO {
            return;
        }

        let local = window.mouse_position() - self.bounds.origin;
        let next = point(
            local.x.clamp(Pixels::ZERO, self.bounds.size.width),
            local.y.clamp(Pixels::ZERO, self.bounds.size.height),
        );

        if self.pointer != next {
            self.pointer = next;
            cx.notify();
        }
    }
}

impl SpotlightCard {
    fn render_overlay(
        state: Entity<SpotlightCardState>,
        accent: Hsla,
        glow: Hsla,
        radius: Pixels,
    ) -> impl IntoElement {
        canvas(
            {
                let state = state.clone();
                move |bounds, _, cx| {
                    state.update(cx, |state, _| {
                        state.set_bounds(bounds);
                    });
                }
            },
            move |bounds, _, window, cx| {
                let state = state.read(cx);
                let left_bar_width = if state.hovered { px(3.0) } else { px(2.0) };
                let left_bar_color = if state.hovered {
                    accent.opacity(0.95)
                } else {
                    accent.opacity(0.42)
                };

                window.paint_quad(
                    fill(
                        Bounds::new(bounds.origin, size(left_bar_width, bounds.size.height)),
                        left_bar_color,
                    )
                    .corner_radii(radius),
                );

                if !state.hovered {
                    return;
                }

                for (diameter, opacity) in [
                    (px(280.0), 0.26),
                    (px(190.0), 0.18),
                    (px(120.0), 0.12),
                ] {
                    let halo_bounds = Bounds::new(
                        point(
                            bounds.origin.x + state.pointer.x - diameter / 2.,
                            bounds.origin.y + state.pointer.y - diameter / 2.,
                        ),
                        size(diameter, diameter),
                    );
                    window.paint_quad(
                        fill(halo_bounds, glow.opacity(opacity)).corner_radii(diameter / 2.),
                    );
                }

                window.paint_quad(outline(bounds, accent.opacity(0.22)).corner_radii(radius));
            },
        )
        .absolute()
        .size_full()
    }
}

impl RenderOnce for SpotlightCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| SpotlightCardState::default());
        let hovered = state.read(cx).hovered;
        let hover_background = self.hover_background;
        let accent = self.accent;
        let glow = self.glow;
        let radius = self.radius;

        div()
            .id(self.id.clone())
            .relative()
            .overflow_hidden()
            .refine_style(&self.style)
            .when(hovered, |this| {
                this.when_some(hover_background, |this, background| this.bg(background))
            })
            .on_hover(window.listener_for(&state, |state, hovered, _, cx| {
                state.set_hovered(*hovered, cx);
            }))
            .on_mouse_move(window.listener_for(&state, |state, _, window, cx| {
                state.update_pointer(window, cx);
            }))
            .on_prepaint({
                let state = state.clone();
                move |bounds, _, cx| {
                    state.update(cx, |state, _| {
                        state.set_bounds(bounds);
                    });
                }
            })
            .child(Self::render_overlay(state, accent, glow, radius))
            .children(self.children)
    }
}
