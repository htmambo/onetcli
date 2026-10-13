// From:
// https://github.com/zed-industries/zed/blob/56daba28d40301ee4c05546fadb691d070b7b2b6/crates/gpui/examples/window_shadow.rs
use gpui::{
    AnyElement, App, Bounds, CursorStyle, Decorations, Edges, HitboxBehavior, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce,
    ResizeEdge, Size, Styled as _, Window, canvas, div, point, prelude::FluentBuilder as _, px,
};

use crate::ActiveTheme;
#[cfg(target_os = "linux")]
use crate::title_bar::linux_prefers_system_window_controls;

#[cfg(not(target_os = "linux"))]
pub(crate) const SHADOW_SIZE: Pixels = px(0.0);
#[cfg(target_os = "linux")]
pub(crate) const SHADOW_SIZE: Pixels = px(12.0);
pub(crate) const BORDER_SIZE: Pixels = px(4.0);

/// Wayland 会话下自绘外框的透明留白宽度。
///
/// Wayland 没有服务端装饰边距，这里留出透明区域承接圆角与自绘阴影。
const WAYLAND_PADDING_SIZE: Pixels = px(2.0);

/// Wayland 会话下的自绘缩放热区宽度。
///
/// 该值等于 [`WAYLAND_PADDING_SIZE`] + [`BORDER_SIZE`]，即完全盖住可见边框。
/// Wayland 下合成器不提供任何缩放边距（`client_inset` 恒为 0），若热区沿用
/// [`WAYLAND_PADDING_SIZE`]，就会出现"看得见边框却拖不动"的错位。
const RESIZE_HIT_SIZE: Pixels = px(6.0);

#[cfg(target_os = "linux")]
fn linux_uses_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[cfg(not(target_os = "linux"))]
fn linux_uses_wayland_session() -> bool {
    false
}

/// Create a new window border.
pub fn window_border() -> WindowBorder {
    WindowBorder::new()
}

/// Window border use to render a custom window border and shadow for Linux.
#[derive(IntoElement)]
pub struct WindowBorder {
    shadow_size: Pixels,
    children: Vec<AnyElement>,
}

impl Default for WindowBorder {
    fn default() -> Self {
        Self {
            shadow_size: SHADOW_SIZE,
            children: Vec::new(),
        }
    }
}

impl WindowBorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the shadow size for typical Linux client-side decorations.
    ///
    /// Default: [`SHADOW_SIZE`]
    pub fn shadow_size(mut self, size: impl Into<Pixels>) -> Self {
        self.shadow_size = size.into();
        self
    }
}

/// Get the window paddings.
pub fn window_paddings(window: &Window) -> Edges<Pixels> {
    let shadow_size = window
        .client_inset_edges()
        .unwrap_or_else(|| Edges::all(window.client_inset().unwrap_or(SHADOW_SIZE)));
    match window.window_decorations() {
        Decorations::Server => Edges::all(px(0.0)),
        Decorations::Client { tiling } => {
            let mut paddings = shadow_size;
            if tiling.top {
                paddings.top = px(0.0);
            }
            if tiling.bottom {
                paddings.bottom = px(0.0);
            }
            if tiling.left {
                paddings.left = px(0.0);
            }
            if tiling.right {
                paddings.right = px(0.0);
            }
            paddings
        }
    }
}

impl ParentElement for WindowBorder {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for WindowBorder {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let decorations = window.window_decorations();
        let shadow_size = self.shadow_size;
        let border_radius = cx.theme().radius_lg;
        #[cfg(target_os = "linux")]
        let prefers_system_frame =
            matches!(decorations, Decorations::Server) || linux_prefers_system_window_controls();
        #[cfg(not(target_os = "linux"))]
        let prefers_system_frame = matches!(decorations, Decorations::Server);
        let shadow_size = if cfg!(target_os = "linux") && linux_uses_wayland_session() {
            WAYLAND_PADDING_SIZE
        } else {
            match decorations {
                Decorations::Client { tiling }
                    if tiling.top && tiling.bottom && tiling.left && tiling.right =>
                {
                    px(0.0)
                }
                _ => shadow_size,
            }
        };
        // 视觉边框宽度与命中热区解耦：若复用 shadow_size，Wayland 下热区会退化成
        // 2px 透明留白，可见的 BORDER_SIZE 边框既拖不动也不显示缩放光标。
        let resize_hit_size = if cfg!(target_os = "linux") && linux_uses_wayland_session() {
            RESIZE_HIT_SIZE
        } else {
            shadow_size
        };
        let client_inset = if prefers_system_frame
            || (cfg!(target_os = "linux") && linux_uses_wayland_session())
        {
            px(0.0)
        } else {
            shadow_size
        };
        let show_content_border =
            prefers_system_frame || matches!(decorations, Decorations::Server);

        // Deepin/X11 的系统标题栏路径下不要继续声明自绘边框范围，
        // 否则窗口管理器可能把窗口当成仍在使用客户端边框。
        if prefers_system_frame {
            window.set_client_inset(px(0.0));
        } else if linux_uses_wayland_session() {
            window.set_client_inset_edges(Edges {
                top: px(0.0),
                right: client_inset,
                bottom: client_inset,
                left: client_inset,
            });
        } else {
            window.set_client_inset(client_inset);
        }

        div()
            .id("window-backdrop")
            .bg(gpui::transparent_black())
            .map(|div| match decorations {
                Decorations::Server => div,
                Decorations::Client { tiling, .. } if !prefers_system_frame => div
                    .bg(gpui::transparent_black())
                    .child(
                        canvas(
                            |_bounds, window, _| {
                                window.insert_hitbox(
                                    Bounds::new(
                                        point(px(0.0), px(0.0)),
                                        window.window_bounds().get_bounds().size,
                                    ),
                                    HitboxBehavior::Normal,
                                )
                            },
                            move |_bounds, hitbox, window, _| {
                                let mouse = window.mouse_position();
                                let size = window.window_bounds().get_bounds().size;
                                let Decorations::Client { tiling } = window.window_decorations()
                                else {
                                    return;
                                };
                                if tiling.top && tiling.bottom && tiling.left && tiling.right {
                                    return;
                                }
                                let Some(edge) = resize_edge(mouse, resize_hit_size, size) else {
                                    return;
                                };
                                window.set_cursor_style(
                                    match edge {
                                        ResizeEdge::Top | ResizeEdge::Bottom => {
                                            CursorStyle::ResizeUpDown
                                        }
                                        ResizeEdge::Left | ResizeEdge::Right => {
                                            CursorStyle::ResizeLeftRight
                                        }
                                        ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                            CursorStyle::ResizeUpLeftDownRight
                                        }
                                        ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                            CursorStyle::ResizeUpRightDownLeft
                                        }
                                    },
                                    &hitbox,
                                );
                            },
                        )
                        .size_full()
                        .absolute(),
                    )
                    .when(!(tiling.top || tiling.right), |div| {
                        div.rounded_tr(border_radius)
                    })
                    .when(!(tiling.top || tiling.left), |div| {
                        div.rounded_tl(border_radius)
                    })
                    .when(!(tiling.bottom || tiling.right), |div| {
                        div.rounded_br(border_radius)
                    })
                    .when(!(tiling.bottom || tiling.left), |div| {
                        div.rounded_bl(border_radius)
                    })
                    .when(!tiling.bottom, |div| div.pt(shadow_size))
                    .when(!tiling.bottom, |div| div.pb(shadow_size))
                    .when(!tiling.left, |div| div.pl(shadow_size))
                    .when(!tiling.right, |div| div.pr(shadow_size))
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let Decorations::Client { tiling } = window.window_decorations() else {
                            return;
                        };
                        if tiling.top && tiling.bottom && tiling.left && tiling.right {
                            return;
                        }
                        let size = window.window_bounds().get_bounds().size;
                        let pos = window.mouse_position();

                        match resize_edge(pos, resize_hit_size, size) {
                            Some(edge) => window.start_window_resize(edge),
                            None => {}
                        };
                    }),
                Decorations::Client { .. } => div,
            })
            .size_full()
            .child(
                div()
                    .cursor(CursorStyle::default())
                    .map(|div| match decorations {
                        Decorations::Server => div,
                        Decorations::Client { tiling } if !prefers_system_frame => div
                            .when(!(tiling.top || tiling.right), |div| {
                                div.rounded_tr(border_radius)
                            })
                            .when(!(tiling.top || tiling.left), |div| {
                                div.rounded_tl(border_radius)
                            })
                            .when(!(tiling.bottom || tiling.right), |div| {
                                div.rounded_br(border_radius)
                            })
                            .when(!(tiling.bottom || tiling.left), |div| {
                                div.rounded_bl(border_radius)
                            })
                            .border_color(cx.theme().border)
                            // .when(!tiling.top && !hide_client_top_border, |div| {
                            //     div.border_t(BORDER_SIZE)
                            // })
                            // .when(!tiling.bottom, |div| div.border_b(BORDER_SIZE))
                            // .when(!tiling.left, |div| div.border_l(BORDER_SIZE))
                            // .when(!tiling.right, |div| div.border_r(BORDER_SIZE))
                            .when(true, |div| div.border(BORDER_SIZE))
                            .when(!tiling.is_tiled() && cx.theme().shadow, |div| {
                                div.shadow(vec![gpui::BoxShadow {
                                    color: Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.14,
                                    },
                                    blur_radius: shadow_size * 0.75,
                                    spread_radius: -shadow_size / 3.,
                                    offset: point(px(0.0), px(0.0)),
                                }])
                            }),
                        Decorations::Client { .. } => div,
                    })
                    // 系统装饰路径下补一层可见内边框，避免窗口边界过弱。
                    .when(show_content_border, |div| {
                        div.border_4()
                            .border_color(cx.theme().border)
                            .rounded(border_radius)
                    })
                    .overflow_hidden()
                    .on_mouse_move(|_e, _, cx| {
                        cx.stop_propagation();
                    })
                    .bg(cx.theme().transparent)
                    .size_full()
                    .children(self.children),
            )
    }
}

fn resize_edge(pos: Point<Pixels>, hit_size: Pixels, size: Size<Pixels>) -> Option<ResizeEdge> {
    let edge = if pos.y < hit_size && pos.x < hit_size {
        ResizeEdge::TopLeft
    } else if pos.y < hit_size && pos.x > size.width - hit_size {
        ResizeEdge::TopRight
    } else if pos.y < hit_size {
        ResizeEdge::Top
    } else if pos.y > size.height - hit_size && pos.x < hit_size {
        ResizeEdge::BottomLeft
    } else if pos.y > size.height - hit_size && pos.x > size.width - hit_size {
        ResizeEdge::BottomRight
    } else if pos.y > size.height - hit_size {
        ResizeEdge::Bottom
    } else if pos.x < hit_size {
        ResizeEdge::Left
    } else if pos.x > size.width - hit_size {
        ResizeEdge::Right
    } else {
        return None;
    };
    Some(edge)
}

#[cfg(test)]
mod tests {
    use super::{BORDER_SIZE, RESIZE_HIT_SIZE, WAYLAND_PADDING_SIZE, resize_edge};
    use gpui::{Pixels, ResizeEdge, Size, point, px, size};

    fn window_size() -> Size<Pixels> {
        size(px(1000.0), px(800.0))
    }

    #[test]
    fn wayland_热区必须覆盖可见边框() {
        // 热区小于"留白 + 可见边框"时，用户瞄准看得见的边框却拖不动窗口。
        assert!(RESIZE_HIT_SIZE >= WAYLAND_PADDING_SIZE + BORDER_SIZE);
    }

    #[test]
    fn 可见边框范围内可以触发缩放() {
        let size = window_size();
        let inside_border = BORDER_SIZE - px(1.0);
        let mid_x = px(500.0);
        let mid_y = px(400.0);

        assert_eq!(
            resize_edge(point(inside_border, mid_y), RESIZE_HIT_SIZE, size),
            Some(ResizeEdge::Left)
        );
        assert_eq!(
            resize_edge(
                point(size.width - inside_border, mid_y),
                RESIZE_HIT_SIZE,
                size
            ),
            Some(ResizeEdge::Right)
        );
        assert_eq!(
            resize_edge(point(mid_x, inside_border), RESIZE_HIT_SIZE, size),
            Some(ResizeEdge::Top)
        );
        assert_eq!(
            resize_edge(
                point(mid_x, size.height - inside_border),
                RESIZE_HIT_SIZE,
                size
            ),
            Some(ResizeEdge::Bottom)
        );
    }

    #[test]
    fn 可见边框的四角可以触发对角缩放() {
        let size = window_size();
        let inside_border = BORDER_SIZE - px(1.0);
        let near_right = size.width - inside_border;
        let near_bottom = size.height - inside_border;

        assert_eq!(
            resize_edge(point(inside_border, inside_border), RESIZE_HIT_SIZE, size),
            Some(ResizeEdge::TopLeft)
        );
        assert_eq!(
            resize_edge(point(near_right, near_bottom), RESIZE_HIT_SIZE, size),
            Some(ResizeEdge::BottomRight)
        );
    }

    #[test]
    fn 内容区域不触发缩放() {
        let size = window_size();
        assert_eq!(
            resize_edge(point(px(500.0), px(400.0)), RESIZE_HIT_SIZE, size),
            None
        );
    }
}
