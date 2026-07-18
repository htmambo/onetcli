use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable};
use one_core::tab_container::{TabContent, TabContentEvent};
use remote_desktop::{
    RemoteDesktopConnectionOptions, RemoteDesktopInput, RemoteDesktopOutput, RemoteDesktopProtocol,
    RemoteDesktopProviderVersionError, RemoteDesktopRuntime, RemoteDesktopSize, RemoteKey,
    RemoteMouseButton, RemoteNamedKey, create_backend,
};
use rust_i18n::t;

use crate::display_mode::DisplayMode;
use crate::ime_guard::RemoteDesktopImeGuard;
use crate::keyboard::keystroke_to_remote_key_for_protocol;
use crate::modifiers::modifier_inputs;
use crate::pixels::{bgra_to_render_image, rgba_to_render_image};
use crate::pointer::{
    LocalBounds, scale_cover_window_pointer_position, scale_filled_window_pointer_position,
    scale_original_pointer_position, scale_window_pointer_position,
};
use crate::shortcuts::{
    ClipboardShortcut, clipboard_shortcut_inputs, is_clipboard_platform_shortcut,
};

const RESIZE_DEBOUNCE: Duration = Duration::from_millis(800);
const RESIZE_MIN_INTERVAL: Duration = Duration::from_millis(1200);
const RESIZE_DELTA_THRESHOLD: u16 = 16;
const CLIPBOARD_SYNC_INTERVAL: Duration = Duration::from_millis(500);
const RDP_DISPLAY_MIN_SIZE: f32 = 200.0;
const RDP_DISPLAY_MAX_SIZE: f32 = 8192.0;
const REMOTE_DESKTOP_CONTEXT: &str = "RemoteDesktopView";

#[cfg(target_os = "macos")]
const REMOTE_COPY_SHORTCUT: &str = "cmd-c";
#[cfg(not(target_os = "macos"))]
const REMOTE_COPY_SHORTCUT: &str = "ctrl-shift-c";
#[cfg(target_os = "macos")]
const REMOTE_PASTE_SHORTCUT: &str = "cmd-v";
#[cfg(not(target_os = "macos"))]
const REMOTE_PASTE_SHORTCUT: &str = "ctrl-shift-v";

actions!(
    remote_desktop_view,
    [SendTab, SendShiftTab, RemoteCopy, RemotePaste]
);

fn remote_desktop_tab_title(title: &str, tab_index: Option<usize>) -> String {
    if let Some(index) = tab_index {
        format!("{title}({index})")
    } else {
        title.to_string()
    }
}

pub struct RemoteDesktopViewConfig {
    pub options: RemoteDesktopConnectionOptions,
    pub title: String,
    pub tab_index: Option<usize>,
}

pub struct RemoteDesktopView {
    options: RemoteDesktopConnectionOptions,
    title: String,
    input_tx: Option<tokio::sync::mpsc::UnboundedSender<RemoteDesktopInput>>,
    output_rx: Option<std::sync::mpsc::Receiver<RemoteDesktopOutput>>,
    focus_handle: FocusHandle,
    frame: Option<Arc<RenderImage>>,
    /// 本地 BGRA 帧缓冲底图（增量帧按矩形 patch 于此），尺寸 = remote_size。
    frame_buffer: Option<(u16, u16, Vec<u8>)>,
    remote_size: Option<(u16, u16)>,
    content_bounds: Option<Bounds<Pixels>>,
    last_resize_size: Option<(u16, u16)>,
    pending_resize_size: Option<(u16, u16)>,
    pending_resize_updated_at: Option<Instant>,
    last_resize_sent_at: Option<Instant>,
    modifiers: Modifiers,
    last_clipboard_text: Option<String>,
    last_clipboard_sync_at: Option<Instant>,
    status: SharedString,
    tab_index: Option<usize>,
    display_mode: DisplayMode,
    /// Original(1:1) 模式下的滚动偏移（逻辑像素）。
    scroll_offset: (f32, f32),
}

impl RemoteDesktopView {
    pub fn new(config: RemoteDesktopViewConfig, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // 事件驱动渲染：有新输出才触发重绘，避免静止时 33ms 空渲染。
        // 另加低频兜底（~500ms），保证剪贴板同步 / resize 防抖等维护任务在无帧时也执行。
        cx.spawn(async move |this, cx| {
            let mut idle_ticks = 0u32;
            loop {
                let result = this.update(cx, |view, cx| {
                    let got_output = view.drain_output(cx);
                    idle_ticks = if got_output {
                        cx.notify();
                        0
                    } else {
                        idle_ticks.saturating_add(1)
                    };
                    // 约 500ms 无输出时兜底重绘一次（驱动剪贴板/resize 维护）。
                    if idle_ticks >= 15 {
                        cx.notify();
                        idle_ticks = 0;
                    }
                });
                // view 已销毁则退出循环，避免后台任务空转泄漏。
                if result.is_err() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
            }
        })
        .detach();

        Self {
            options: config.options,
            title: config.title,
            input_tx: None,
            output_rx: None,
            focus_handle,
            frame: None,
            frame_buffer: None,
            remote_size: None,
            content_bounds: None,
            last_resize_size: None,
            pending_resize_size: None,
            pending_resize_updated_at: None,
            last_resize_sent_at: None,
            modifiers: Modifiers::default(),
            last_clipboard_text: None,
            last_clipboard_sync_at: None,
            status: SharedString::from("Waiting for layout"),
            tab_index: config.tab_index,
            display_mode: DisplayMode::default(),
            scroll_offset: (0.0, 0.0),
        }
    }

    fn start_runtime(&mut self, size: (u16, u16)) {
        if self.input_tx.is_some() {
            return;
        }
        let runtime = create_backend(self.options.clone())
            .start(RemoteDesktopSize {
                width: size.0,
                height: size.1,
            })
            .unwrap_or_else(failed_runtime);
        self.input_tx = Some(runtime.input_tx);
        self.output_rx = Some(runtime.output_rx);
        self.last_resize_size = Some(size);
        self.status = SharedString::from("Connecting");
    }

    /// 收取并处理一波后端输出。返回本轮是否收到任何输出（用于决定是否触发重绘）。
    ///
    /// 性能关键：一 tick 内可能到达多个增量帧。本函数先把它们全部 patch 进底图，
    /// 最后**只构造一次 RenderImage**（避免每帧整帧 clone 导致的累积卡顿）。
    fn drain_output(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(output_rx) = self.output_rx.as_ref() else {
            return false;
        };
        let mut outputs = Vec::new();
        while let Ok(output) = output_rx.try_recv() {
            outputs.push(output);
        }
        let received = !outputs.is_empty();
        // 底图是否被本 tick 的帧更新（决定是否需要重建 RenderImage）。
        let mut frame_dirty = false;
        // RDP 的 RGBA 整帧（不走增量底图路径），仅保留最新一帧。
        let mut latest_rgba_frame: Option<(u16, u16, Vec<u8>)> = None;

        for output in outputs {
            match output {
                RemoteDesktopOutput::Connected { width, height, .. } => {
                    self.remote_size = Some((width, height));
                    self.frame_buffer = None; // 重连/尺寸变化后等首帧整帧重建底图
                    self.status = SharedString::from("Connected");
                }
                RemoteDesktopOutput::Frame {
                    width,
                    height,
                    rgba,
                } => {
                    self.remote_size = Some((width, height));
                    latest_rgba_frame = Some((width, height, rgba));
                }
                RemoteDesktopOutput::FrameBgra {
                    width,
                    height,
                    bgra,
                } => {
                    self.remote_size = Some((width, height));
                    // 整帧作为底图（move 存入），不在此渲染，末尾统一构造一次。
                    self.frame_buffer = Some((width, height, bgra));
                    frame_dirty = true;
                }
                RemoteDesktopOutput::FrameRectsBgra {
                    width,
                    height,
                    rects,
                    bgra,
                } => {
                    self.remote_size = Some((width, height));
                    // 兜底：底图缺失或尺寸不符时惰性分配零底图，
                    // 避免首帧即增量帧 / 契约未满足时永远黑屏。
                    let need_alloc = match &self.frame_buffer {
                        Some((w, h, _)) => *w != width || *h != height,
                        None => true,
                    };
                    if need_alloc {
                        tracing::warn!(
                            width,
                            height,
                            "lazy-allocated zero base frame; helper should send a full frame first"
                        );
                        let len = width as usize * height as usize * 4;
                        self.frame_buffer = Some((width, height, vec![0u8; len]));
                    }
                    // 只 patch 底图，不在此渲染。
                    if let Some((_, _, buffer)) = self.frame_buffer.as_mut() {
                        match patch_bgra_rects(buffer, width, &rects, &bgra) {
                            Ok(()) => frame_dirty = true,
                            Err(error) => {
                                tracing::error!(%error, "patch_bgra_rects failed");
                                self.status = SharedString::from(error.to_string());
                            }
                        }
                    }
                }
                RemoteDesktopOutput::Status(message) => self.status = SharedString::from(message),
                RemoteDesktopOutput::ConnectionFailure(message)
                | RemoteDesktopOutput::Terminated(message) => {
                    self.handle_disconnect_status(message)
                }
                RemoteDesktopOutput::CursorDefault
                | RemoteDesktopOutput::CursorHidden
                | RemoteDesktopOutput::CursorPosition { .. } => {}
                RemoteDesktopOutput::ClipboardText { text } => {
                    if self.last_clipboard_text.as_deref() != Some(text.as_str()) {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        self.last_clipboard_text = Some(text);
                        self.last_clipboard_sync_at = Some(Instant::now());
                    }
                }
            }
        }

        // 统一渲染：本 tick 内所有帧 patch 已合并进底图，只构造一次 RenderImage。
        // 避免每个增量帧都整帧 clone + notify 导致的累积卡顿。
        if let Some((width, height, rgba)) = latest_rgba_frame {
            // RDP 路径：直接渲染最新 RGBA 整帧（不走增量底图）。
            match rgba_to_render_image(width, height, rgba) {
                Ok(image) => self.frame = Some(Arc::new(image)),
                Err(error) => {
                    tracing::error!(%error, "rgba_to_render_image failed");
                    self.status = SharedString::from(error.to_string());
                }
            }
        } else if frame_dirty {
            match self.render_frame_buffer() {
                Ok(image) => self.frame = Some(Arc::new(image)),
                Err(error) => {
                    tracing::error!(%error, "render_frame_buffer failed");
                    self.status = SharedString::from(error.to_string());
                }
            }
        }
        received
    }

    /// 从本地底图渲染当前帧。
    fn render_frame_buffer(&self) -> anyhow::Result<RenderImage> {
        let (width, height, buffer) = self
            .frame_buffer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no frame buffer"))?;
        bgra_to_render_image(*width, *height, buffer.clone())
    }

    fn sync_local_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        if self
            .last_clipboard_sync_at
            .is_some_and(|synced_at| synced_at.elapsed() < CLIPBOARD_SYNC_INTERVAL)
        {
            return;
        }
        self.last_clipboard_sync_at = Some(Instant::now());
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if self.last_clipboard_text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_clipboard_text = Some(text.clone());
        self.send_input(RemoteDesktopInput::ClipboardText { text });
    }

    fn handle_disconnect_status(&mut self, message: String) {
        self.modifiers = Modifiers::default();
        self.status = SharedString::from(message);
    }

    fn request_reconnect(&mut self) {
        self.modifiers = Modifiers::default();
        self.status = SharedString::from("reconnecting RDP session");
        self.send_input(RemoteDesktopInput::Reconnect);
    }

    fn update_content_bounds(&mut self, bounds: Bounds<Pixels>, display_scale_factor: f32) {
        self.content_bounds = Some(bounds);
        let Some(size) = resize_dimensions_from_bounds_with_scale(bounds, display_scale_factor)
        else {
            return;
        };
        self.start_runtime(size);
        if !is_meaningful_resize_delta(self.last_resize_size, size) {
            return;
        }
        if self.pending_resize_size == Some(size) {
            return;
        }
        self.pending_resize_size = Some(size);
        self.pending_resize_updated_at = Some(Instant::now());
    }

    fn flush_pending_resize(&mut self) {
        if self.remote_size.is_none() {
            return;
        }
        let Some(size) = self.pending_resize_size else {
            return;
        };
        let Some(updated_at) = self.pending_resize_updated_at else {
            return;
        };
        if updated_at.elapsed() < RESIZE_DEBOUNCE {
            return;
        }
        if self
            .last_resize_sent_at
            .is_some_and(|sent_at| sent_at.elapsed() < RESIZE_MIN_INTERVAL)
        {
            return;
        }
        self.pending_resize_size = None;
        self.pending_resize_updated_at = None;
        self.last_resize_size = Some(size);
        self.last_resize_sent_at = Some(Instant::now());
        self.send_input(RemoteDesktopInput::Resize {
            width: size.0,
            height: size.1,
        });
    }

    fn send_input(&self, input: RemoteDesktopInput) {
        if self.options.read_only
            && !matches!(
                input,
                RemoteDesktopInput::Resize { .. } | RemoteDesktopInput::Reconnect
            )
        {
            return;
        }

        if let Some(input_tx) = &self.input_tx {
            let _ = input_tx.send(input);
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if is_clipboard_platform_shortcut(&event.keystroke) {
            cx.stop_propagation();
            return;
        }
        if let Some(key) =
            keystroke_to_remote_key_for_protocol(&event.keystroke, self.options.protocol)
        {
            self.send_input(RemoteDesktopInput::Key { key, pressed: true });
        }
        cx.stop_propagation();
    }

    fn handle_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if is_clipboard_platform_shortcut(&event.keystroke) {
            cx.stop_propagation();
            return;
        }
        if let Some(key) =
            keystroke_to_remote_key_for_protocol(&event.keystroke, self.options.protocol)
        {
            self.send_input(RemoteDesktopInput::Key {
                key,
                pressed: false,
            });
        }
        cx.stop_propagation();
    }

    fn send_tab(&mut self, _: &SendTab, _window: &mut Window, cx: &mut Context<Self>) {
        self.send_key_press(RemoteKey::Named(RemoteNamedKey::Tab));
        cx.stop_propagation();
    }

    fn send_shift_tab(&mut self, _: &SendShiftTab, _window: &mut Window, cx: &mut Context<Self>) {
        self.send_input(RemoteDesktopInput::Key {
            key: RemoteKey::Named(RemoteNamedKey::Shift),
            pressed: true,
        });
        self.send_key_press(RemoteKey::Named(RemoteNamedKey::Tab));
        self.send_input(RemoteDesktopInput::Key {
            key: RemoteKey::Named(RemoteNamedKey::Shift),
            pressed: false,
        });
        cx.stop_propagation();
    }

    fn remote_copy(&mut self, _: &RemoteCopy, _window: &mut Window, cx: &mut Context<Self>) {
        self.send_clipboard_shortcut(ClipboardShortcut::Copy);
        cx.stop_propagation();
    }

    fn remote_paste(&mut self, _: &RemotePaste, _window: &mut Window, cx: &mut Context<Self>) {
        self.send_local_clipboard_to_remote(cx);
        self.send_clipboard_shortcut(ClipboardShortcut::Paste);
        cx.stop_propagation();
    }

    fn send_key_press(&self, key: RemoteKey) {
        self.send_input(RemoteDesktopInput::Key {
            key: key.clone(),
            pressed: true,
        });
        self.send_input(RemoteDesktopInput::Key {
            key,
            pressed: false,
        });
    }

    fn send_clipboard_shortcut(&self, shortcut: ClipboardShortcut) {
        for input in clipboard_shortcut_inputs(self.options.protocol, shortcut) {
            self.send_input(input);
        }
    }

    fn send_local_clipboard_to_remote(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.last_clipboard_text = Some(text.clone());
        self.last_clipboard_sync_at = Some(Instant::now());
        self.send_input(RemoteDesktopInput::ClipboardText { text });
    }

    fn handle_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.modifiers;
        self.modifiers = event.modifiers;

        if self.options.protocol == RemoteDesktopProtocol::Rdp {
            for input in modifier_inputs(previous, event.modifiers) {
                self.send_input(input);
            }
        }

        cx.stop_propagation();
    }

    fn send_pointer_move(&mut self, position: Point<Pixels>, window: &mut Window) {
        let Some((remote_width, remote_height)) = self.remote_size else {
            return;
        };
        let bounds = self.pointer_bounds(window);
        let px_x = pixels_to_f32(position.x);
        let px_y = pixels_to_f32(position.y);
        let mapped = match self.display_mode {
            DisplayMode::Fill => {
                scale_filled_window_pointer_position(px_x, px_y, bounds, remote_width, remote_height)
            }
            DisplayMode::Contain => {
                scale_window_pointer_position(px_x, px_y, bounds, remote_width, remote_height)
            }
            DisplayMode::Cover => {
                scale_cover_window_pointer_position(px_x, px_y, bounds, remote_width, remote_height)
            }
            DisplayMode::Original => scale_original_pointer_position(
                px_x - bounds.left,
                px_y - bounds.top,
                self.scroll_offset.0,
                self.scroll_offset.1,
                remote_width,
                remote_height,
            ),
        };
        let Some((x, y)) = mapped else {
            return;
        };
        self.send_input(RemoteDesktopInput::MouseMove { x, y });
    }

    fn send_mouse_button(&self, button: MouseButton, pressed: bool) {
        let Some(button) = map_mouse_button(button) else {
            return;
        };
        self.send_input(RemoteDesktopInput::MouseButton { button, pressed });
    }

    fn send_scroll(&mut self, event: &ScrollWheelEvent) {
        let (dx, dy) = match event.delta {
            ScrollDelta::Lines(delta) => (delta.x * 100.0, delta.y * 100.0),
            ScrollDelta::Pixels(delta) => (pixels_to_f32(delta.x), pixels_to_f32(delta.y)),
        };
        // Original(1:1) 模式下滚轮用于滚动视图，而不是发给远端。
        if self.display_mode.is_scrollable() {
            self.scroll_view(dx, dy);
            return;
        }
        self.send_scroll_delta(dx, dy, 1.0);
    }

    /// Original(1:1) 模式下滚动视图：调整偏移并限制在可滚动范围内。
    fn scroll_view(&mut self, dx: f32, dy: f32) {
        let Some((remote_w, remote_h)) = self.remote_size else {
            return;
        };
        let Some(bounds) = self.content_bounds else {
            return;
        };
        let view_w = pixels_to_f32(bounds.size.width);
        let view_h = pixels_to_f32(bounds.size.height);
        let max_x = (f32::from(remote_w) - view_w).max(0.0);
        let max_y = (f32::from(remote_h) - view_h).max(0.0);
        // 滚轮向下（dy>0）内容向上移动 = 偏移增大。
        self.scroll_offset.0 = (self.scroll_offset.0 + dx).clamp(0.0, max_x);
        self.scroll_offset.1 = (self.scroll_offset.1 + dy).clamp(0.0, max_y);
    }

    fn send_scroll_delta(&self, x: f32, y: f32, multiplier: f32) {
        if x.abs() > 0.001 {
            self.send_input(RemoteDesktopInput::Wheel {
                vertical: false,
                units: (x * multiplier) as i16,
            });
        }
        if y.abs() > 0.001 {
            self.send_input(RemoteDesktopInput::Wheel {
                vertical: true,
                units: (y * multiplier) as i16,
            });
        }
    }

    fn set_display_mode(&mut self, mode: DisplayMode, cx: &mut Context<Self>) {
        if self.display_mode == mode {
            return;
        }
        self.display_mode = mode;
        if !mode.is_scrollable() {
            self.scroll_offset = (0.0, 0.0);
        }
        cx.notify();
    }

    /// 顶部显示模式切换条：分段按钮，当前模式高亮。
    fn display_mode_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.display_mode;
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("RemoteDesktop.display_mode.label").to_string()),
            )
            .children(DisplayMode::ALL.into_iter().map(|mode| {
                let button = Button::new(format!("display-mode-{}", mode.id_key()))
                    .xsmall()
                    .label(mode.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_display_mode(mode, cx);
                    }));
                if mode == current {
                    button.primary()
                } else {
                    button.ghost()
                }
            }))
    }

    fn pointer_bounds(&self, window: &mut Window) -> LocalBounds {
        self.content_bounds.map(bounds_to_local).unwrap_or_else(|| {
            let size = window.viewport_size();
            LocalBounds {
                left: 0.0,
                top: 0.0,
                width: pixels_to_f32(size.width),
                height: pixels_to_f32(size.height),
            }
        })
    }
}

fn map_mouse_button(button: MouseButton) -> Option<RemoteMouseButton> {
    match button {
        MouseButton::Left => Some(RemoteMouseButton::Left),
        MouseButton::Right => Some(RemoteMouseButton::Right),
        MouseButton::Middle => Some(RemoteMouseButton::Middle),
        MouseButton::Navigate(NavigationDirection::Back) => Some(RemoteMouseButton::X1),
        MouseButton::Navigate(NavigationDirection::Forward) => Some(RemoteMouseButton::X2),
    }
}

/// 把一组脏矩形的 BGRA 数据 patch 进底图（行优先，bgra 为各矩形顺序拼接）。
/// 纯函数便于单测；越界/截断返回错误。
fn patch_bgra_rects(
    buffer: &mut [u8],
    frame_width: u16,
    rects: &[remote_desktop::helper_protocol::FrameRect],
    bgra: &[u8],
) -> anyhow::Result<()> {
    let frame_width = frame_width as usize;
    let mut offset = 0usize;
    for rect in rects {
        let rect_w = rect.width as usize;
        let len = rect_w * rect.height as usize * 4;
        anyhow::ensure!(offset + len <= bgra.len(), "rects payload truncated");
        let data = &bgra[offset..offset + len];
        offset += len;
        for row in 0..rect.height as usize {
            let dst = ((rect.y as usize + row) * frame_width + rect.x as usize) * 4;
            let src_start = row * rect_w * 4;
            let src_end = src_start + rect_w * 4;
            anyhow::ensure!(dst + rect_w * 4 <= buffer.len(), "rect outside framebuffer");
            buffer[dst..dst + rect_w * 4].copy_from_slice(&data[src_start..src_end]);
        }
    }
    Ok(())
}

fn pixels_to_f32(pixels: Pixels) -> f32 {
    pixels.into()
}

fn bounds_to_local(bounds: Bounds<Pixels>) -> LocalBounds {
    LocalBounds {
        left: pixels_to_f32(bounds.left()),
        top: pixels_to_f32(bounds.top()),
        width: pixels_to_f32(bounds.size.width),
        height: pixels_to_f32(bounds.size.height),
    }
}

fn resize_dimensions_from_bounds_with_scale(
    bounds: Bounds<Pixels>,
    display_scale_factor: f32,
) -> Option<(u16, u16)> {
    let display_scale_factor = if display_scale_factor.is_finite() && display_scale_factor > 0.0 {
        display_scale_factor
    } else {
        1.0
    };

    let mut width = (pixels_to_f32(bounds.size.width) * display_scale_factor)
        .round()
        .clamp(RDP_DISPLAY_MIN_SIZE, RDP_DISPLAY_MAX_SIZE) as u16;
    if width % 2 != 0 {
        width = width.saturating_sub(1);
    }
    let height = (pixels_to_f32(bounds.size.height) * display_scale_factor)
        .round()
        .clamp(RDP_DISPLAY_MIN_SIZE, RDP_DISPLAY_MAX_SIZE) as u16;
    Some((width, height))
}

fn is_meaningful_resize_delta(previous: Option<(u16, u16)>, next: (u16, u16)) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    previous.0.abs_diff(next.0) >= RESIZE_DELTA_THRESHOLD
        || previous.1.abs_diff(next.1) >= RESIZE_DELTA_THRESHOLD
}

fn failed_runtime(error: anyhow::Error) -> RemoteDesktopRuntime {
    let (input_tx, _input_rx) = tokio::sync::mpsc::unbounded_channel();
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    let _ = output_tx.send(RemoteDesktopOutput::ConnectionFailure(
        remote_desktop_error_message(&error),
    ));
    RemoteDesktopRuntime {
        input_tx,
        output_rx,
    }
}

fn remote_desktop_error_message(error: &anyhow::Error) -> String {
    if let Some(error) = error.downcast_ref::<RemoteDesktopProviderVersionError>() {
        let key = if error.invalid {
            "RemoteDesktop.provider_version_invalid"
        } else {
            "RemoteDesktop.provider_version_too_old"
        };
        return t!(
            key,
            protocol = error.protocol.label(),
            installed = error.installed.as_str(),
            required = error.required.as_str()
        )
        .to_string();
    }
    error.to_string()
}

impl Focusable for RemoteDesktopView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TabContentEvent> for RemoteDesktopView {}

impl TabContent for RemoteDesktopView {
    fn content_key(&self) -> &'static str {
        "RemoteDesktop"
    }

    fn title(&self, _cx: &App) -> SharedString {
        SharedString::from(remote_desktop_tab_title(&self.title, self.tab_index))
    }

    fn icon(&self, _cx: &App) -> Option<Icon> {
        Some(match self.options.protocol {
            RemoteDesktopProtocol::Rdp => IconName::Rdp.color(),
            RemoteDesktopProtocol::Vnc => IconName::Vnc.color(),
        })
    }

    fn closeable(&self, _cx: &App) -> bool {
        true
    }

    fn try_close(
        &mut self,
        _tab_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<bool> {
        if let Some(input_tx) = &self.input_tx {
            let _ = input_tx.send(RemoteDesktopInput::Close);
        }
        Task::ready(true)
    }
}

impl Render for RemoteDesktopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // drain_output 只在后台 timer 任务中执行（单一消费者，避免与 render 竞争 channel）。
        self.sync_local_clipboard(window, cx);
        self.flush_pending_resize();
        let view = cx.entity();
        let focus_handle = self.focus_handle.clone();
        let show_status_overlay = self.status.as_ref() != "Connected";

        let content = div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .key_context(REMOTE_DESKTOP_CONTEXT)
            .on_action(cx.listener(Self::send_tab))
            .on_action(cx.listener(Self::send_shift_tab))
            .on_action(cx.listener(Self::remote_copy))
            .on_action(cx.listener(Self::remote_paste))
            .capture_key_down(cx.listener(Self::handle_key_down))
            .capture_key_up(cx.listener(Self::handle_key_up))
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.send_pointer_move(event.position, window);
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, true);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, true);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, true);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, false);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, false);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.send_pointer_move(event.position, window);
                    this.send_mouse_button(event.button, false);
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                this.send_scroll(event);
                cx.stop_propagation();
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        window.handle_input(&focus_handle, RemoteDesktopImeGuard::new(bounds), cx);
                    },
                )
                .absolute()
                .size_full(),
            )
            .when_some(self.frame.clone(), |this, frame| {
                match self.display_mode {
                    DisplayMode::Original => {
                        // 原始尺寸 1:1：图像保持远端分辨率，用负偏移实现滚动。
                        let (ox, oy) = self.scroll_offset;
                        this.child(
                            div().size_full().overflow_hidden().child(
                                img(frame)
                                    .object_fit(ObjectFit::None)
                                    .absolute()
                                    .left(px(-ox))
                                    .top(px(-oy)),
                            ),
                        )
                    }
                    mode => this.child(img(frame).size_full().object_fit(mode.object_fit())),
                }
            })
            .when(self.frame.is_none(), |this| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.status.clone()),
                )
            });

        let frame_area = div()
            .size_full()
            .relative()
            .on_children_prepainted(move |bounds, window, cx| {
                if let Some(bounds) = bounds.first().copied() {
                    view.update(cx, |view, _| {
                        view.update_content_bounds(bounds, window.scale_factor());
                    });
                }
            })
            .child(content)
            .when(show_status_overlay, |this| {
                this.child(
                    div()
                        .id("remote-desktop-status-overlay")
                        .absolute()
                        .top_2()
                        .left_2()
                        .max_w(px(520.0))
                        .px_3()
                        .py_1()
                        .border_1()
                        .rounded_sm()
                        .bg(cx.theme().background)
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.request_reconnect();
                            cx.stop_propagation();
                        }))
                        .child(self.status.clone()),
                )
            });

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.display_mode_toolbar(cx))
            .child(div().flex_1().min_h_0().child(frame_area))
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", SendTab, Some(REMOTE_DESKTOP_CONTEXT)),
        KeyBinding::new("shift-tab", SendShiftTab, Some(REMOTE_DESKTOP_CONTEXT)),
        KeyBinding::new(
            REMOTE_COPY_SHORTCUT,
            RemoteCopy,
            Some(REMOTE_DESKTOP_CONTEXT),
        ),
        KeyBinding::new(
            REMOTE_PASTE_SHORTCUT,
            RemotePaste,
            Some(REMOTE_DESKTOP_CONTEXT),
        ),
    ]);
}

pub fn refresh_keybindings(_cx: &mut App) {}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};
    use remote_desktop::helper_protocol::FrameRect;
    use remote_desktop::{RemoteDesktopProtocol, RemoteDesktopProviderVersionError};

    fn rect(x: u16, y: u16, w: u16, h: u16) -> FrameRect {
        FrameRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn patch_bgra_rects_writes_single_rect() {
        // 3x1 底图，patch 中间 1x1 矩形。
        let mut buffer = vec![0u8; 3 * 4];
        let rects = [rect(1, 0, 1, 1)];
        let data = [9u8, 8, 7, 255];

        super::patch_bgra_rects(&mut buffer, 3, &rects, &data).unwrap();

        assert_eq!(
            buffer,
            vec![0, 0, 0, 0, 9, 8, 7, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn patch_bgra_rects_writes_multiple_rects_in_order() {
        // 4x1 底图，两个矩形拼接数据。
        let mut buffer = vec![0u8; 4 * 4];
        let rects = [rect(0, 0, 1, 1), rect(3, 0, 1, 1)];
        let data = [1u8, 0, 0, 255, 2, 0, 0, 255];

        super::patch_bgra_rects(&mut buffer, 4, &rects, &data).unwrap();

        assert_eq!(
            buffer,
            vec![1, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 255]
        );
    }

    #[test]
    fn patch_bgra_rects_handles_tall_rect() {
        // 2x2 底图，patch 整列 (x=0,w=1,h=2)。
        let mut buffer = vec![0u8; 2 * 2 * 4];
        let rects = [rect(0, 0, 1, 2)];
        let data = [5u8, 0, 0, 255, 6, 0, 0, 255];

        super::patch_bgra_rects(&mut buffer, 2, &rects, &data).unwrap();

        assert_eq!(
            buffer,
            vec![5, 0, 0, 255, 0, 0, 0, 0, 6, 0, 0, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn patch_bgra_rects_rejects_truncated_payload() {
        let mut buffer = vec![0u8; 4 * 4];
        let rects = [rect(0, 0, 2, 2)]; // 需 16 字节
        let data = [0u8; 8]; // 只有 8

        assert!(super::patch_bgra_rects(&mut buffer, 2, &rects, &data).is_err());
    }

    #[test]
    fn patch_bgra_rects_rejects_out_of_bounds() {
        let mut buffer = vec![0u8; 2 * 4];
        let rects = [rect(5, 0, 2, 1)]; // x 越界

        assert!(super::patch_bgra_rects(&mut buffer, 2, &rects, &[0u8; 8]).is_err());
    }

    #[test]
    fn resize_dimensions_from_bounds_adjust_to_display_control_limits() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1281.4), px(720.6)));

        assert_eq!(
            Some((1280, 721)),
            super::resize_dimensions_from_bounds_with_scale(bounds, 1.0)
        );

        let oversized = Bounds::new(point(px(0.0), px(0.0)), size(px(90000.0), px(0.0)));

        assert_eq!(
            Some((8192, 200)),
            super::resize_dimensions_from_bounds_with_scale(oversized, 1.0)
        );
    }

    #[test]
    fn resize_dimensions_from_bounds_applies_display_scale_factor() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)));

        assert_eq!(
            Some((3840, 2160)),
            super::resize_dimensions_from_bounds_with_scale(bounds, 2.0)
        );
    }

    #[test]
    fn resize_change_requires_meaningful_delta() {
        assert!(!super::is_meaningful_resize_delta(
            Some((1280, 720)),
            (1284, 726)
        ));
        assert!(super::is_meaningful_resize_delta(
            Some((1280, 720)),
            (1300, 726)
        ));
        assert!(super::is_meaningful_resize_delta(None, (1280, 720)));
    }

    #[test]
    fn tab_title_uses_connection_name_and_duplicate_index() {
        assert_eq!(
            "prod-rdp",
            super::remote_desktop_tab_title("prod-rdp", None)
        );
        assert_eq!(
            "prod-rdp(2)",
            super::remote_desktop_tab_title("prod-rdp", Some(2))
        );
    }

    #[test]
    fn provider_version_error_message_is_localized_after_context() {
        let error = anyhow::Error::new(RemoteDesktopProviderVersionError {
            protocol: RemoteDesktopProtocol::Vnc,
            installed: "0.1.0".to_string(),
            required: "0.1.1".to_string(),
            invalid: false,
        })
        .context("VNC remote desktop provider");

        let message = super::remote_desktop_error_message(&error);

        assert_eq!(
            "VNC provider version 0.1.0 is too old. Please update the provider to 0.1.1 or newer.",
            message
        );
    }
}
