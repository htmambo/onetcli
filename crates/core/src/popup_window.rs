use std::sync::Arc;

use gpui::{
    AnyView, App, AppContext, Bounds, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, SharedString, Size, Styled, Subscription,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, actions, div, px,
    size,
};
use gpui_component::{
    ActiveTheme as _, FocusTrapElement, Root, TitleBar, app_style, modal_surface_palette, v_flex,
};

actions!(popup_window, [CancelPopup]);

const CONTEXT: &str = "PopupWindow";

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelPopup, Some(CONTEXT))]);
}

pub fn request_popup_window_close(window: &mut Window, cx: &mut App) {
    window.defer(cx, |window, _| {
        window.remove_window();
    });
}

#[derive(Clone, Copy)]
struct PopupParentMetrics {
    bounds: Bounds<gpui::Pixels>,
    viewport_size: Size<gpui::Pixels>,
}

fn popup_parent_metrics(window: &Window) -> PopupParentMetrics {
    PopupParentMetrics {
        bounds: window.window_bounds().get_bounds(),
        viewport_size: window.viewport_size(),
    }
}

fn clamp_popup_content_size(
    requested_size: Size<gpui::Pixels>,
    max_content_size: Size<gpui::Pixels>,
) -> Size<gpui::Pixels> {
    size(
        requested_size.width.min(max_content_size.width),
        requested_size.height.min(max_content_size.height),
    )
}

fn centered_popup_bounds_in_parent(
    requested_size: Size<gpui::Pixels>,
    parent_bounds: Bounds<gpui::Pixels>,
) -> Bounds<gpui::Pixels> {
    Bounds::centered_at(parent_bounds.center(), requested_size)
}

fn centered_popup_bounds(
    requested_size: Size<gpui::Pixels>,
    parent_bounds: Option<Bounds<gpui::Pixels>>,
    cx: &App,
) -> Bounds<gpui::Pixels> {
    if let Some(bounds) = parent_bounds {
        return centered_popup_bounds_in_parent(requested_size, bounds);
    }

    if let Some(display) = cx.primary_display() {
        let visible_bounds = display.visible_bounds();
        let clamped_size = clamp_popup_content_size(requested_size, visible_bounds.size);
        return Bounds::centered_at(visible_bounds.center(), clamped_size);
    }

    Bounds::centered(None, requested_size, cx)
}

fn popup_window_background(cx: &App) -> WindowBackgroundAppearance {
    #[cfg(target_os = "macos")]
    {
        return WindowBackgroundAppearance::Blurred;
    }

    if cx.theme().window_blur_enabled {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

struct PopupWindowView {
    focus_handle: FocusHandle,
    content: AnyView,
    max_content_size: Option<Size<gpui::Pixels>>,
    _bounds_subscription: Option<Subscription>,
}

impl PopupWindowView {
    fn new(
        content: AnyView,
        max_content_size: Option<Size<gpui::Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let bounds_subscription = max_content_size.map(|_| {
            cx.observe_window_bounds(window, |this, window, cx| {
                this.enforce_window_size_limit(window, cx);
            })
        });

        let mut view = Self {
            focus_handle,
            content,
            max_content_size,
            _bounds_subscription: bounds_subscription,
        };
        view.enforce_window_size_limit(window, cx);
        view
    }

    fn on_cancel_popup(&mut self, _: &CancelPopup, window: &mut Window, cx: &mut Context<Self>) {
        request_popup_window_close(window, cx);
    }

    fn enforce_window_size_limit(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        let Some(max_content_size) = self.max_content_size else {
            return;
        };

        let current_size = window.viewport_size();
        let clamped_size = clamp_popup_content_size(current_size, max_content_size);
        if clamped_size != current_size {
            window.resize(clamped_size);
        }
    }
}

impl Focusable for PopupWindowView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PopupWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface_palette = modal_surface_palette(cx.theme());

        v_flex()
            .id("popup-window-root")
            .size_full()
            .bg(surface_palette.content)
            .border_1()
            .border_color(app_style::border_strong())
            .rounded(cx.theme().radius_lg)
            .text_color(app_style::text())
            .overflow_hidden()
            // Root 已经改为透明，仅负责承接窗口级圆角与阴影；
            // popup 壳层自己仍保留圆角与矩形 overflow mask，用于约束壳层背景和滚动区域。
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .focus_trap("popup-window-root", &self.focus_handle)
            .on_action(cx.listener(Self::on_cancel_popup))
            .child(div().size_full().child(self.content.clone()))
    }
}

/// 弹出窗口的配置选项
pub struct PopupWindowOptions {
    pub title: SharedString,
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub kind: WindowKind,
}

impl Default for PopupWindowOptions {
    fn default() -> Self {
        Self {
            title: "".into(),
            width: 600.0,
            height: 550.0,
            min_width: 400.0,
            min_height: 300.0,
            kind: WindowKind::Dialog,
        }
    }
}

impl PopupWindowOptions {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn min_width(mut self, min_width: f32) -> Self {
        self.min_width = min_width;
        self
    }

    pub fn min_height(mut self, min_height: f32) -> Self {
        self.min_height = min_height;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn kind(mut self, kind: WindowKind) -> Self {
        self.kind = kind;
        self
    }
}

/// 创建弹出窗口
///
/// 异步创建一个独立的弹出窗口，窗口内容由 `create_view_fn` 提供。
/// 窗口会自动包含 Root 组件以支持 notification 等功能。
///
/// # 参数
/// - `options`: 窗口配置选项
/// - `create_view_fn`: 创建窗口内容的闭包
/// - `cx`: App 上下文
///
/// # 示例
/// ```ignore
/// open_popup_window(
///     PopupWindowOptions::new("My Window").size(600.0, 400.0),
///     |window, cx| {
///         cx.new(|cx| MyView::new(window, cx))
///     },
///     cx,
/// );
/// ```
pub fn open_popup_window<F, E>(
    parent_window: &mut Window,
    options: PopupWindowOptions,
    create_view_fn: F,
    cx: &mut App,
) where
    E: Into<AnyView>,
    F: FnOnce(&mut Window, &mut App) -> E + Send + 'static,
{
    open_popup_window_with_should_close(
        parent_window,
        options,
        create_view_fn,
        |window, cx| {
            request_popup_window_close(window, cx);
            false
        },
        cx,
    );
}

pub fn open_popup_window_with_should_close<F, E, H>(
    parent_window: &mut Window,
    options: PopupWindowOptions,
    create_view_fn: F,
    on_should_close: H,
    cx: &mut App,
) where
    E: Into<AnyView>,
    F: FnOnce(&mut Window, &mut App) -> E + Send + 'static,
    H: Fn(&mut Window, &mut App) -> bool + Send + Sync + 'static,
{
    let parent_metrics = popup_parent_metrics(parent_window);
    let parent_window_handle = parent_window.window_handle();
    let requested_min_size = Size {
        width: px(options.min_width),
        height: px(options.min_height),
    };
    let min_size = clamp_popup_content_size(requested_min_size, parent_metrics.viewport_size);
    let requested_content_size = size(
        px(options.width).max(min_size.width),
        px(options.height).max(min_size.height),
    );
    let content_size =
        clamp_popup_content_size(requested_content_size, parent_metrics.viewport_size);
    let window_bounds = centered_popup_bounds(content_size, Some(parent_metrics.bounds), cx);
    let title = options.title.clone();
    let on_should_close = Arc::new(on_should_close);
    let kind = options.kind.clone();
    let corner_radius = cx.theme().radius_lg;
    let window_background = popup_window_background(cx);

    cx.spawn(async move |cx| {
        let on_should_close = Arc::clone(&on_should_close);
        let _ = parent_window_handle.update(cx, |_, window, _| {
            window.activate_window();
        });
        let window_opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            // Dialog windows on macOS are created as sheets (beginSheet) which
            // don't have standard window buttons — passing a titlebar with
            // traffic_light_position causes a nil pointer dereference in
            // MacWindowState::move_traffic_light.
            titlebar: if matches!(kind, WindowKind::Dialog) {
                None
            } else {
                Some(TitleBar::title_bar_options())
            },
            window_min_size: Some(min_size),
            kind,
            window_background,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };

        let window = cx.open_window(window_opts, move |window, cx| {
            let on_should_close = Arc::clone(&on_should_close);
            window.on_window_should_close(cx, move |window, cx| on_should_close(window, cx));
	    // 消除linux中可能出现的窗口直角
            window.set_blur_behind_corner_radius(corner_radius);
            let view = create_view_fn(window, cx).into();
            let popup_view =
                cx.new(|cx| PopupWindowView::new(view, Some(content_size), window, cx));
            cx.new(|cx| {
                let mut root = Root::new(popup_view, window, cx);
                #[cfg(target_os = "linux")]
                {
                    // popup 的可见底色由 PopupWindowView 承担，避免 Root 底色在圆角处透出。
                    root = root.bg(gpui::transparent_black());
                }
                root
            })
        })?;

        window.update(cx, |_, window, _| {
            window.activate_window();
            window.set_window_title(&title);
        })?;

        Ok::<_, anyhow::Error>(())
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point};

    #[test]
    fn popup_window_defaults_to_dialog_kind() {
        assert_eq!(PopupWindowOptions::default().kind, WindowKind::Dialog);
    }

    #[test]
    fn popup_content_size_is_clamped_to_parent_viewport() {
        let clamped =
            clamp_popup_content_size(size(px(900.0), px(720.0)), size(px(640.0), px(480.0)));

        assert_eq!(clamped, size(px(640.0), px(480.0)));
    }

    #[test]
    fn popup_bounds_are_centered_inside_parent_window() {
        let parent_bounds = Bounds {
            origin: point(px(100.0), px(80.0)),
            size: size(px(1200.0), px(800.0)),
        };

        let bounds = centered_popup_bounds_in_parent(size(px(640.0), px(480.0)), parent_bounds);

        assert_eq!(
            bounds,
            Bounds {
                origin: point(px(380.0), px(240.0)),
                size: size(px(640.0), px(480.0)),
            }
        );
    }
}
