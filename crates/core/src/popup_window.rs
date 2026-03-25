use gpui::{
    AnyView, App, AppContext, Bounds, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, SharedString, Size, Styled, Window,
    WindowBounds, WindowKind, WindowOptions, actions, div, px, size,
};
use gpui_component::{FocusTrapElement, Root, TitleBar, app_style, v_flex};

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

struct PopupWindowView {
    focus_handle: FocusHandle,
    content: AnyView,
}

impl PopupWindowView {
    fn new(content: AnyView, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self {
            focus_handle,
            content,
        }
    }

    fn on_cancel_popup(&mut self, _: &CancelPopup, window: &mut Window, cx: &mut Context<Self>) {
        request_popup_window_close(window, cx);
    }
}

impl Focusable for PopupWindowView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PopupWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("popup-window-root")
            .size_full()
            .bg(app_style::page_bg())
            .border_1()
            .border_color(app_style::border_strong())
            .text_color(app_style::text_primary())
            .overflow_hidden()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .focus_trap("popup-window-root", &self.focus_handle)
            .on_action(cx.listener(Self::on_cancel_popup))
            .child(
                div()
                    .size_full()
                    .bg(app_style::page_bg())
                    .child(self.content.clone()),
            )
    }
}

/// 弹出窗口的配置选项
pub struct PopupWindowOptions {
    pub title: SharedString,
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub min_height: f32,
}

impl Default for PopupWindowOptions {
    fn default() -> Self {
        Self {
            title: "".into(),
            width: 600.0,
            height: 550.0,
            min_width: 400.0,
            min_height: 300.0,
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
pub fn open_popup_window<F, E>(options: PopupWindowOptions, create_view_fn: F, cx: &mut App)
where
    E: Into<AnyView>,
    F: FnOnce(&mut Window, &mut App) -> E + Send + 'static,
{
    let mut window_size = size(px(options.width), px(options.height));
    if let Some(display) = cx.primary_display() {
        let display_size = display.bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.85);
        window_size.height = window_size.height.min(display_size.height * 0.85);
    }
    let window_bounds = Bounds::centered(None, window_size, cx);
    let title = options.title.clone();

    cx.spawn(async move |cx| {
        let window_opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            titlebar: Some(TitleBar::title_bar_options()),
            window_min_size: Some(Size {
                width: px(options.min_width),
                height: px(options.min_height),
            }),
            kind: WindowKind::Normal,
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };

        let window = cx.open_window(window_opts, |window, cx| {
            window.on_window_should_close(cx, |window, cx| {
                request_popup_window_close(window, cx);
                false
            });
            let view = create_view_fn(window, cx).into();
            let popup_view = cx.new(|cx| PopupWindowView::new(view, window, cx));
            cx.new(|cx| Root::new(popup_view, window, cx))
        })?;

        window.update(cx, |_, window, _| {
            window.activate_window();
            window.set_window_title(&title);
        })?;

        Ok::<_, anyhow::Error>(())
    })
    .detach();
}
