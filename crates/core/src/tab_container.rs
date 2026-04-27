use crate::{PendingChangeLevel, RunningState};
use futures::future::{Either, select};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyView, App, AppContext as _, Context, Corner, Decorations, Entity, EntityId, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Render, RenderOnce, ScrollWheelEvent, SharedString, Styled, Subscription, Task, TextRun,
    Window, WindowControlArea, div, px,
};
use gpui::{ScrollHandle, StatefulInteractiveElement as _};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_component::list::{List, ListDelegate, ListState};
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::popover::Popover;
use gpui_component::{
    ActiveTheme, Colorize, Icon, IconName, IndexPath, InteractiveElementExt as _, Selectable,
    Sizable, Size, WindowExt as _, WindowsSurfaceLayer, h_flex, layered_level_surface_color,
    linux_prefers_system_window_controls, should_render_custom_window_controls, v_flex,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ============================================================================
// TabContainer Events
// ============================================================================

/// Events emitted by TabContent
#[derive(Debug, Clone)]
pub enum TabContentEvent {
    /// Tab state changed
    StateChanged,
}

/// Events emitted by TabContainer
#[derive(Debug, Clone)]
pub enum TabContainerEvent {
    /// Layout has changed (tabs added, removed, reordered, or active index changed)
    LayoutChanged,
    /// Active content state has changed and may affect title or status summaries.
    ActiveContentChanged,
    /// A tab was activated
    TabActivated { index: usize, id: String },
    /// A tab was closed
    TabClosed { id: String },
    /// Request the owner to open an SFTP view for the SSH tab.
    OpenSftpRequested { tab_id: String },
    /// Request the owner to trigger the trailing tab-bar action.
    TabBarTrailingActionRequested,
}

// ============================================================================
// State Serialization Structures
// ============================================================================

/// Serializable state for TabContainer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabContainerState {
    /// Version for compatibility checking
    #[serde(default)]
    pub version: Option<usize>,
    /// All tab states
    pub tabs: Vec<TabItemState>,
    /// Currently active tab index
    pub active_index: usize,
    /// Container UI configuration
    #[serde(default)]
    pub config: TabContainerConfig,
}

impl Default for TabContainerState {
    fn default() -> Self {
        Self {
            version: Some(1),
            tabs: Vec::new(),
            active_index: 0,
            config: TabContainerConfig::default(),
        }
    }
}

/// Serializable state for a single tab
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabItemState {
    /// Unique tab ID
    pub id: SharedString,
    /// Tab From
    pub from: SharedString,
    /// Tab key
    pub key: SharedString,
    /// Tab-specific data (customized by each content type)
    #[serde(default)]
    pub data: serde_json::Value,
}

/// UI configuration for TabContainer
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TabContainerConfig {
    /// Tab size: "xsmall", "small", "medium", "large"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Left padding in pixels
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_padding: Option<f32>,
    /// Top padding in pixels
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_padding: Option<f32>,
}

// ============================================================================
// TabContent Trait - Static Type Interface (like Panel)
// ============================================================================

/// Trait that defines tab content behavior.
/// Implement this on your Entity type (like Panel).
/// Requires: Render + Focusable + EventEmitter<TabContentEvent>
#[allow(unused_variables)]
pub trait TabContent: EventEmitter<TabContentEvent> + Render + Focusable {
    /// Unique key for this content type (used for serialization)
    fn content_key(&self) -> &'static str;

    /// Get the tab title
    fn title(&self, cx: &App) -> SharedString;

    /// Get optional icon for the tab
    fn icon(&self, cx: &App) -> Option<Icon> {
        None
    }

    /// Get optional status summary for global status bars.
    fn status_summary(&self, cx: &App) -> Option<SharedString> {
        None
    }

    /// Get optional rich-element status summary for global status bars.
    /// Returned elements are rendered directly (supports icons).
    fn status_summary_element(&self, _cx: &App) -> Option<gpui::AnyElement> {
        None
    }

    /// Get optional subtitle shown below tab title (e.g., current path for terminals)
    fn subtitle(&self, cx: &App) -> Option<SharedString> {
        None
    }

    /// Check if tab can be closed
    fn closeable(&self, cx: &App) -> bool {
        true
    }

    /// Called when tab becomes active
    fn on_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {}

    /// Called when tab becomes inactive
    fn on_deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {}

    /// Try to close this tab. Returns a Task that resolves to true if close succeeded.
    fn try_close(
        &mut self,
        tab_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        Task::ready(true)
    }

    /// Force close this tab during an application-level confirmed exit.
    /// By default it reuses the normal close path.
    fn force_close(
        &mut self,
        tab_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        self.try_close(tab_id, window, cx)
    }

    /// Get running state for this tab. Returns Some if there are blocking operations
    /// (e.g., running process, active transfer) that require user confirmation before close.
    fn running_state(&self, cx: &App) -> Option<RunningState> {
        None
    }

    /// Check if this tab has pending changes that need to be committed (e.g., unsaved database edits).
    fn has_pending_changes(&self, cx: &App) -> bool {
        false
    }

    /// Get the pending change level for visual indication (Delete > Modify > Insert)
    fn pending_change_level(&self, cx: &App) -> Option<PendingChangeLevel> {
        None
    }

    /// Get tab's preferred width size
    fn width_size(&self, cx: &App) -> Option<Size> {
        None
    }

    /// Dump tab state to serializable data
    fn dump(&self, cx: &App) -> serde_json::Value {
        serde_json::Value::Null
    }

    /// 在应用退出前调用，允许 tab 执行 detach 等轻量清理操作。
    fn prepare_for_app_quit(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}

// ============================================================================
// TabContentView Trait - Dynamic Type Interface (like PanelView)
// ============================================================================

/// Dynamic trait object interface for TabContent.
/// This allows storing different TabContent types in a single collection.
#[allow(unused_variables)]
pub trait TabContentView: 'static + Send + Sync {
    fn content_key(&self, cx: &App) -> &'static str;
    fn content_id(&self, cx: &App) -> EntityId;
    fn title(&self, cx: &App) -> SharedString;
    fn icon(&self, cx: &App) -> Option<Icon>;
    fn status_summary(&self, cx: &App) -> Option<SharedString>;
    fn status_summary_element(&self, cx: &App) -> Option<gpui::AnyElement>;
    fn subtitle(&self, cx: &App) -> Option<SharedString>;
    fn closeable(&self, cx: &App) -> bool;
    fn on_activate(&self, window: &mut Window, cx: &mut App);
    fn on_deactivate(&self, window: &mut Window, cx: &mut App);
    fn try_close(&self, tab_id: &str, window: &mut Window, cx: &mut App) -> Task<bool>;
    fn force_close(&self, tab_id: &str, window: &mut Window, cx: &mut App) -> Task<bool>;
    fn prepare_for_app_quit(&self, tab_id: &str, window: &mut Window, cx: &mut App);
    fn running_state(&self, cx: &App) -> Option<RunningState>;
    fn has_pending_changes(&self, cx: &App) -> bool;
    fn pending_change_level(&self, cx: &App) -> Option<PendingChangeLevel>;
    fn width_size(&self, cx: &App) -> Option<Size>;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn view(&self) -> AnyView;
    fn dump(&self, cx: &App) -> serde_json::Value;
    fn subscribe_state(&self, cx: &mut Context<TabContainer>) -> Subscription;
}

/// Blanket implementation: Entity<T: TabContent> automatically implements TabContentView
impl<T: TabContent> TabContentView for Entity<T> {
    fn content_key(&self, cx: &App) -> &'static str {
        self.read(cx).content_key()
    }

    fn content_id(&self, _cx: &App) -> EntityId {
        self.entity_id()
    }

    fn title(&self, cx: &App) -> SharedString {
        self.read(cx).title(cx)
    }

    fn icon(&self, cx: &App) -> Option<Icon> {
        self.read(cx).icon(cx)
    }

    fn status_summary(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).status_summary(cx)
    }

    fn status_summary_element(&self, cx: &App) -> Option<gpui::AnyElement> {
        self.read(cx).status_summary_element(cx)
    }

    fn subtitle(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).subtitle(cx)
    }

    fn closeable(&self, cx: &App) -> bool {
        self.read(cx).closeable(cx)
    }

    fn on_activate(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.on_activate(window, cx))
    }

    fn on_deactivate(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.on_deactivate(window, cx))
    }

    fn try_close(&self, tab_id: &str, window: &mut Window, cx: &mut App) -> Task<bool> {
        let tab_id = tab_id.to_string();
        self.update(cx, |this, cx| this.try_close(&tab_id, window, cx))
    }

    fn force_close(&self, tab_id: &str, window: &mut Window, cx: &mut App) -> Task<bool> {
        let tab_id = tab_id.to_string();
        self.update(cx, |this, cx| this.force_close(&tab_id, window, cx))
    }

    fn prepare_for_app_quit(&self, tab_id: &str, window: &mut Window, cx: &mut App) {
        let _ = tab_id;
        self.update(cx, |this, cx| this.prepare_for_app_quit(window, cx));
    }

    fn running_state(&self, cx: &App) -> Option<RunningState> {
        self.read(cx).running_state(cx)
    }

    fn has_pending_changes(&self, cx: &App) -> bool {
        self.read(cx).has_pending_changes(cx)
    }

    fn pending_change_level(&self, cx: &App) -> Option<PendingChangeLevel> {
        self.read(cx).pending_change_level(cx)
    }

    fn width_size(&self, cx: &App) -> Option<Size> {
        self.read(cx).width_size(cx)
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn dump(&self, cx: &App) -> serde_json::Value {
        self.read(cx).dump(cx)
    }

    fn subscribe_state(&self, cx: &mut Context<TabContainer>) -> Subscription {
        cx.subscribe(self, |this, entity, _: &TabContentEvent, cx| {
            if this.active_content_id(cx) == Some(entity.entity_id()) {
                cx.emit(TabContainerEvent::ActiveContentChanged);
            }
            cx.notify();
        })
    }
}

impl From<&dyn TabContentView> for AnyView {
    fn from(handle: &dyn TabContentView) -> Self {
        handle.view()
    }
}

fn should_suppress_duplicate_status_summary(content_key: &str) -> bool {
    content_key != "Terminal"
}

impl PartialEq for dyn TabContentView {
    fn eq(&self, other: &Self) -> bool {
        self.view() == other.view()
    }
}

// ============================================================================
// TabItem - Represents a single tab with its content
// ============================================================================

pub struct TabItem {
    id: SharedString,
    from: SharedString,
    content: Arc<dyn TabContentView>,
}

impl TabItem {
    pub fn new<T: TabContent>(
        id: impl Into<String>,
        from: impl Into<String>,
        content: Entity<T>,
    ) -> Self {
        Self {
            id: SharedString::from(id.into()),
            from: SharedString::from(from.into()),
            content: Arc::new(content),
        }
    }

    pub fn id(&self) -> SharedString {
        self.id.clone()
    }

    pub fn from(&self) -> SharedString {
        self.from.clone()
    }

    pub fn content(&self) -> &Arc<dyn TabContentView> {
        &self.content
    }
}

// ============================================================================
// TabContentBuilder - Factory trait for rebuilding tabs
// ============================================================================

/// Trait for building TabContent from serialized state
pub trait TabContentBuilder: Send + Sync {
    fn build(
        &self,
        state: &TabItemState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn TabContentView>>;
}

/// Function-based builder wrapper
pub struct FnTabContentBuilder<F>(pub F);

impl<F> TabContentBuilder for FnTabContentBuilder<F>
where
    F: Fn(&TabItemState, &mut Window, &mut App) -> Option<Arc<dyn TabContentView>> + Send + Sync,
{
    fn build(
        &self,
        state: &TabItemState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn TabContentView>> {
        self.0(state, window, cx)
    }
}

// ============================================================================
// TabContentRegistry - Registry for rebuilding tabs from state
// ============================================================================

/// Registry for TabContent builders, used to restore tabs from saved state
#[derive(Clone)]
pub struct TabContentRegistry {
    builders: HashMap<SharedString, Arc<dyn TabContentBuilder>>,
}

impl Default for TabContentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TabContentRegistry {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
        }
    }

    /// Register a builder for a content type
    pub fn register<B: TabContentBuilder + 'static>(
        &mut self,
        content_type: SharedString,
        builder: B,
    ) {
        self.builders.insert(content_type, Arc::new(builder));
    }

    /// Register a builder using a closure
    pub fn register_fn<F>(&mut self, key: SharedString, builder: F)
    where
        F: Fn(&TabItemState, &mut Window, &mut App) -> Option<Arc<dyn TabContentView>>
            + Send
            + Sync
            + 'static,
    {
        self.builders
            .insert(key, Arc::new(FnTabContentBuilder(builder)));
    }

    /// Build a TabContentView from state
    pub fn build(
        &self,
        state: &TabItemState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn TabContentView>> {
        self.builders.get(&state.key)?.build(state, window, cx)
    }

    /// Check if a builder exists for a content type
    pub fn has_builder(&self, key: &str) -> bool {
        self.builders.contains_key(key)
    }
}

/// Global wrapper for TabContentRegistry
impl gpui::Global for TabContentRegistry {}

// ============================================================================
// TabBarDragState - Window drag state management
// ============================================================================

const WINDOWS_TAB_BAR_DRAG_SPACER_WIDTH: Pixels = px(72.0);
const TAB_BAR_HEIGHT: Pixels = px(33.0);
pub const WINDOW_CONTROL_BUTTON_SIZE: Pixels = px(32.0);

/// 窗口拖动状态，用于在非 Windows 平台支持手动拖动窗口
struct TabBarDragState {
    should_move: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabBarDragPlan {
    enable_scroll_area_drag: bool,
    enable_single_pinned_tab_drag: bool,
}

fn build_tab_bar_drag_plan(
    show_window_controls: bool,
    has_pinned_tab: bool,
    has_scrollable_tabs: bool,
) -> TabBarDragPlan {
    if !show_window_controls {
        return TabBarDragPlan {
            enable_scroll_area_drag: false,
            enable_single_pinned_tab_drag: false,
        };
    }

    TabBarDragPlan {
        enable_scroll_area_drag: true,
        enable_single_pinned_tab_drag: has_pinned_tab && !has_scrollable_tabs,
    }
}

impl Render for TabBarDragState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// 协调 tab 区域和 scroll region 的鼠标交互，防止 scroll region 的窗口拖动
/// 干扰 tab 的点击和拖动事件。
struct TabBarInteractionState {
    /// 标记当前是否有活跃的 tab 点击事件
    tab_click_active: bool,
}

impl Render for TabBarInteractionState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn uses_manual_window_move(show_window_controls: bool, is_windows: bool) -> bool {
    show_window_controls && !is_windows
}

fn should_render_windows_drag_spacer(show_window_controls: bool, is_windows: bool) -> bool {
    show_window_controls && is_windows
}

fn should_render_inline_drag_spacer(
    show_window_controls: bool,
    is_windows: bool,
    is_linux: bool,
) -> bool {
    show_window_controls && (is_windows || is_linux)
}

const TAB_REORDER_DRAG_THRESHOLD: f64 = 6.0;
const TAB_ITEM_GAP: Pixels = px(8.0);
const TAB_HORIZONTAL_PADDING: Pixels = px(24.0);
const TAB_ICON_WIDTH: Pixels = px(16.0);
const TAB_CLOSE_BUTTON_WIDTH: Pixels = px(16.0);
const TAB_PENDING_INDICATOR_WIDTH: Pixels = px(8.0);
const TAB_COMPACT_ITEM_GAP: Pixels = px(4.0);
const TAB_TITLE_TEXT_SCALE_MACOS: f32 = 0.875;
const TAB_TITLE_TEXT_SCALE_LINUX: f32 = 1.0;
const TAB_TITLE_TEXT_SCALE_WINDOWS: f32 = 1.275;
const TAB_TITLE_TEXT_REFERENCE_FONT_SIZE: f32 = 18.0;
const TAB_TITLE_TEXT_LARGE_FONT_EXTRA_PER_PX: f32 = 0.015;
const TAB_TITLE_TEXT_MAX_LARGE_FONT_EXTRA: f32 = 0.45;

fn tab_chrome_width(has_icon: bool, has_pending_indicator: bool, closeable: bool) -> Pixels {
    let mut width = TAB_HORIZONTAL_PADDING;

    if has_icon {
        width += TAB_ICON_WIDTH + TAB_ITEM_GAP;
    }

    if has_pending_indicator {
        width += TAB_PENDING_INDICATOR_WIDTH + TAB_COMPACT_ITEM_GAP;
        if closeable {
            width += TAB_CLOSE_BUTTON_WIDTH + TAB_COMPACT_ITEM_GAP;
        }
    } else if closeable {
        width += TAB_CLOSE_BUTTON_WIDTH + TAB_COMPACT_ITEM_GAP;
    }

    width
}

fn tab_title_text_scale(is_macos: bool, is_windows: bool) -> f32 {
    if is_macos {
        TAB_TITLE_TEXT_SCALE_MACOS
    } else if is_windows {
        TAB_TITLE_TEXT_SCALE_WINDOWS
    } else {
        TAB_TITLE_TEXT_SCALE_LINUX
    }
}

fn tab_title_measure_font_size(
    theme_font_size: Pixels,
    is_macos: bool,
    is_windows: bool,
) -> Pixels {
    let base_scale = tab_title_text_scale(is_macos, is_windows);
    let large_font_extra = ((f32::from(theme_font_size) - TAB_TITLE_TEXT_REFERENCE_FONT_SIZE)
        .max(0.0)
        * TAB_TITLE_TEXT_LARGE_FONT_EXTRA_PER_PX)
        .min(TAB_TITLE_TEXT_MAX_LARGE_FONT_EXTRA);

    theme_font_size * base_scale * (1.0 + large_font_extra)
}

// ============================================================================
// DragTab - Visual representation during drag
// ============================================================================

/// Represents a tab being dragged, used for visual feedback
#[derive(Clone)]
pub struct DragTab {
    pub tab_index: usize,
    pub title: SharedString,
}

impl DragTab {
    pub fn new(tab_index: usize, title: SharedString) -> Self {
        Self { tab_index, title }
    }
}

impl Render for DragTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("drag-tab")
            .cursor_grabbing()
            .py_1()
            .px(px(8.0))
            .min_w(px(80.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.0))
            .text_color(cx.theme().tab_foreground)
            .bg(cx.theme().tab_active)
            .opacity(0.85)
            .shadow_md()
            .text_sm()
            .child(self.title.clone())
    }
}

// ============================================================================
// TabListItem - Custom list item for tab dropdown
// ============================================================================

#[derive(IntoElement)]
pub struct TabListItem {
    tab_index: usize,
    title: SharedString,
    icon: Option<Icon>,
    closeable: bool,
    selected: bool,
    container: Entity<TabContainer>,
}

#[derive(IntoElement)]
pub struct TabListActionItem {
    label: SharedString,
    selected: bool,
}

impl TabListActionItem {
    pub fn new(label: SharedString) -> Self {
        Self {
            label,
            selected: false,
        }
    }
}

impl Selectable for TabListActionItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for TabListActionItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .id("tab-list-action-item")
            .w_full()
            .px_2()
            .py_1()
            .rounded(px(4.0))
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(self.selected, |el| el.bg(cx.theme().list_active))
            .when(!self.selected, |el| {
                el.hover(|style| style.bg(cx.theme().list_hover))
            })
            .child(
                Icon::new(IconName::Plus)
                    .size_4()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label),
            )
    }
}

#[derive(IntoElement)]
pub enum TabListPopoverItem {
    Action(TabListActionItem),
    Tab(TabListItem),
}

impl Selectable for TabListPopoverItem {
    fn selected(mut self, selected: bool) -> Self {
        match &mut self {
            Self::Action(item) => item.selected = selected,
            Self::Tab(item) => item.selected = selected,
        }
        self
    }

    fn is_selected(&self) -> bool {
        match self {
            Self::Action(item) => item.selected,
            Self::Tab(item) => item.selected,
        }
    }
}

impl RenderOnce for TabListPopoverItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match self {
            Self::Action(item) => item.render(window, cx).into_any_element(),
            Self::Tab(item) => item.render(window, cx).into_any_element(),
        }
    }
}

impl TabListItem {
    pub fn new(
        tab_index: usize,
        title: SharedString,
        icon: Option<Icon>,
        closeable: bool,
        selected: bool,
        container: Entity<TabContainer>,
    ) -> Self {
        Self {
            tab_index,
            title,
            icon,
            closeable,
            selected,
            container,
        }
    }
}

impl Selectable for TabListItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for TabListItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let container = self.container.clone();
        let tab_index = self.tab_index;
        let selected = self.selected;
        let drag_border_color = cx.theme().drag_border;
        let drag_title = self.title.clone();

        h_flex()
            .id(SharedString::from(format!("tab-item-{}", tab_index)))
            .w_full()
            .px_2()
            .py_1()
            .rounded(px(4.0))
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(selected, |el| el.bg(cx.theme().list_active))
            .when(!selected, |el| {
                el.hover(|style| style.bg(cx.theme().list_hover))
            })
            .drag_threshold(TAB_REORDER_DRAG_THRESHOLD)
            .on_drag(
                DragTab::new(tab_index, drag_title),
                |drag, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.new(|_| drag.clone())
                },
            )
            .drag_over::<DragTab>(move |el, _, _, _cx| {
                el.border_t_2().border_color(drag_border_color)
            })
            .on_drop(
                window.listener_for(&container, move |this, drag: &DragTab, window, cx| {
                    let from_index = drag.tab_index;
                    let to_index = tab_index;
                    if from_index == to_index {
                        return;
                    }
                    this.move_tab(from_index, to_index, cx);
                    this.set_active_index(to_index, window, cx);
                    if let Some(tab_list) = &this.tab_list {
                        let tabs_data: Vec<(usize, SharedString, Option<Icon>, bool)> = this
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(idx, tab)| {
                                (
                                    idx,
                                    tab.content().title(cx),
                                    tab.content().icon(cx),
                                    tab.content().closeable(cx),
                                )
                            })
                            .collect();
                        tab_list.update(cx, |state, cx| {
                            let delegate = state.delegate_mut();
                            delegate.tabs = tabs_data.clone();
                            delegate.filtered_tabs = tabs_data;
                            cx.notify();
                        });
                    }
                }),
            )
            .when_some(self.icon, |el, icon| {
                el.child(
                    Icon::new(icon)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.title),
            )
            .when(self.closeable, |el| {
                let container = container.clone();
                el.child(
                    div()
                        .id(SharedString::from(format!("close-btn-{}", tab_index)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(16.0))
                        .h(px(16.0))
                        .rounded(px(2.0))
                        .cursor_pointer()
                        .text_color(cx.theme().muted_foreground)
                        .hover(|style| style.bg(cx.theme().muted).text_color(cx.theme().foreground))
                        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                            container.update(cx, |this, cx| {
                                this.close_tab(tab_index, window, cx).detach();
                            });
                        })
                        .child("×"),
                )
            })
    }
}

// ============================================================================
// TabListDelegate - List delegate for tab dropdown
// ============================================================================

pub struct TabListDelegate {
    container: Entity<TabContainer>,
    tabs: Vec<(usize, SharedString, Option<Icon>, bool)>,
    filtered_tabs: Vec<(usize, SharedString, Option<Icon>, bool)>,
    header_action_label: Option<SharedString>,
    selected_index: Option<IndexPath>,
}

impl ListDelegate for TabListDelegate {
    type Item = TabListPopoverItem;

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        if query.is_empty() {
            self.filtered_tabs = self.tabs.clone();
        } else {
            let query_lower = query.to_lowercase();
            self.filtered_tabs = self
                .tabs
                .iter()
                .filter(|(_, title, _, _)| title.to_lowercase().contains(&query_lower))
                .cloned()
                .collect();
        }
        cx.notify();
        Task::ready(())
    }

    fn sections_count(&self, _cx: &App) -> usize {
        if self.header_action_label.is_some() {
            2
        } else {
            1
        }
    }

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        match (self.header_action_label.is_some(), _section) {
            (true, 0) => 1,
            (true, 1) => self.filtered_tabs.len(),
            (false, 0) => self.filtered_tabs.len(),
            _ => 0,
        }
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        if ix.section == 0 {
            if let Some(label) = self.header_action_label.clone() {
                return Some(TabListPopoverItem::Action(TabListActionItem::new(label)));
            }
        }

        let tab_row = if self.header_action_label.is_some() {
            if ix.section != 1 {
                return None;
            }
            ix.row
        } else {
            ix.row
        };
        let (tab_index, title, icon, closeable) = self.filtered_tabs.get(tab_row)?.clone();
        let container = self.container.read(cx);
        let active_index = container.active_index();
        let is_active =
            is_regular_tab_active(tab_index, active_index, container.is_pinned_tab_active());

        Some(TabListPopoverItem::Tab(TabListItem::new(
            tab_index,
            title,
            icon,
            closeable,
            is_active,
            self.container.clone(),
        )))
    }

    fn render_section_footer(
        &mut self,
        section: usize,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        if self.header_action_label.is_some() && section == 0 {
            Some(
                div()
                    .w_full()
                    .h(px(1.0))
                    .mx_2()
                    .bg(cx.theme().border)
                    .into_any_element(),
            )
        } else {
            None::<gpui::AnyElement>
        }
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if let Some(ix) = self.selected_index {
            if self.header_action_label.is_some() && ix.section == 0 {
                self.container.update(cx, |this, cx| {
                    this.set_tab_list_popover_open(false, window, cx);
                    this.request_tab_bar_trailing_action(cx);
                });
                return;
            }

            if let Some((tab_index, _, _, _)) = self.filtered_tabs.get(ix.row) {
                let tab_index = *tab_index;
                self.container.update(cx, |this, cx| {
                    this.list_popover_open = false;
                    this.set_active_index(tab_index, window, cx);
                });
            }
        }
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<ListState<Self>>) {
        self.container.update(cx, |this, cx| {
            this.list_popover_open = false;
            cx.notify();
        });
    }
}

// ============================================================================
// TabContainer - Main container component
// ============================================================================

fn inactive_tab_background_alpha(surface_opacity: f32, tab_bar_alpha: f32) -> f32 {
    let extra = if tab_bar_alpha < 1.0 { 0.4 } else { 0.2 };
    (surface_opacity + extra).clamp(0.0, 1.0)
}

fn is_regular_tab_active(tab_index: usize, active_index: usize, pinned_tab_active: bool) -> bool {
    !pinned_tab_active && tab_index == active_index
}

fn with_alpha(color: gpui::Hsla, alpha: f32) -> gpui::Hsla {
    gpui::Hsla {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

fn shift_tab_tone(color: gpui::Hsla, is_dark: bool, amount: f32) -> gpui::Hsla {
    let adjusted = if is_dark {
        color.lighten(amount)
    } else {
        color.darken(amount)
    };

    gpui::Hsla {
        l: adjusted.l.clamp(0.0, 1.0),
        ..adjusted
    }
}

fn default_inactive_tab_color(
    tab_bar_color: gpui::Hsla,
    surface_opacity: f32,
    is_dark: bool,
) -> gpui::Hsla {
    let target_alpha =
        inactive_tab_background_alpha(surface_opacity, tab_bar_color.a).max(tab_bar_color.a);

    with_alpha(shift_tab_tone(tab_bar_color, is_dark, 0.18), target_alpha)
}

fn resolve_tab_bar_color(
    explicit_tab_bar_color: Option<gpui::Hsla>,
    theme_tab_bar_color: gpui::Hsla,
    blur_enabled: bool,
    backdrop_opacity: f32,
) -> gpui::Hsla {
    explicit_tab_bar_color.unwrap_or_else(|| {
        layered_level_surface_color(
            theme_tab_bar_color,
            blur_enabled,
            backdrop_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        )
    })
}

fn resolve_inactive_tab_color(
    explicit_inactive_tab_color: Option<gpui::Hsla>,
    explicit_tab_bar_color: Option<gpui::Hsla>,
    theme_tab_color: gpui::Hsla,
    resolved_tab_bar_color: gpui::Hsla,
    surface_opacity: f32,
    is_dark: bool,
    blur_enabled: bool,
    backdrop_opacity: f32,
) -> gpui::Hsla {
    explicit_inactive_tab_color.unwrap_or_else(|| {
        if explicit_tab_bar_color.is_some() {
            default_inactive_tab_color(resolved_tab_bar_color, surface_opacity, is_dark)
        } else {
            layered_level_surface_color(
                theme_tab_color,
                blur_enabled,
                backdrop_opacity,
                2,
                WindowsSurfaceLayer::ContentBase,
            )
        }
    })
}

fn default_inactive_tab_border_color(
    inactive_tab_color: gpui::Hsla,
    border_color: gpui::Hsla,
    is_dark: bool,
) -> gpui::Hsla {
    let contrasted = shift_tab_tone(inactive_tab_color, is_dark, 0.28);
    let themed_border = shift_tab_tone(border_color, is_dark, 0.08);
    let blended = contrasted.mix(themed_border, 0.65);

    with_alpha(blended, border_color.a.max(inactive_tab_color.a))
}

pub struct TabContainer {
    focus_handle: FocusHandle,
    tabs: Vec<TabItem>,
    active_index: usize,
    size: Size,
    show_menu: bool,
    tab_bar_bg_color: Option<gpui::Hsla>,
    tab_bar_border_color: Option<gpui::Hsla>,
    active_tab_bg_color: Option<gpui::Hsla>,
    inactive_tab_hover_color: Option<gpui::Hsla>,
    inactive_tab_bg_color: Option<gpui::Hsla>,
    inactive_tab_border_color: Option<gpui::Hsla>,
    tab_text_color: Option<gpui::Hsla>,
    tab_close_button_color: Option<gpui::Hsla>,
    left_padding: Option<gpui::Pixels>,
    top_padding: Option<gpui::Pixels>,
    tab_bar_scroll_handle: ScrollHandle,
    list_popover_open: bool,
    tab_list: Option<Entity<ListState<TabListDelegate>>>,
    closing_tabs: HashSet<SharedString>,
    show_window_controls: bool,
    window_close_handler: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    tab_bar_trailing_view: Option<AnyView>,
    tab_list_header_action_label: Option<SharedString>,
    /// Pinned tab that stays fixed before the scrollable tab list
    pinned_tab: Option<TabItem>,
    /// Whether the pinned tab is currently active (showing its content)
    pinned_tab_active: bool,
    content_subscriptions: HashMap<EntityId, Subscription>,
}

impl EventEmitter<TabContainerEvent> for TabContainer {}

impl TabContainer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = window;
        Self {
            focus_handle: cx.focus_handle(),
            tabs: Vec::new(),
            active_index: 0,
            size: Size::Large,
            show_menu: false,
            tab_bar_bg_color: None,
            tab_bar_border_color: None,
            active_tab_bg_color: None,
            inactive_tab_hover_color: None,
            inactive_tab_bg_color: None,
            inactive_tab_border_color: None,
            tab_text_color: None,
            tab_close_button_color: None,
            left_padding: None,
            top_padding: None,
            tab_bar_scroll_handle: ScrollHandle::new(),
            list_popover_open: false,
            tab_list: None,
            closing_tabs: HashSet::new(),
            show_window_controls: false,
            window_close_handler: None,
            tab_bar_trailing_view: None,
            tab_list_header_action_label: None,
            pinned_tab: None,
            pinned_tab_active: false,
            content_subscriptions: HashMap::new(),
        }
    }

    pub fn with_inactive_tab_bg_color(mut self, color: impl Into<Option<gpui::Hsla>>) -> Self {
        self.inactive_tab_bg_color = color.into();
        self
    }

    pub fn with_inactive_tab_border_color(mut self, color: impl Into<Option<gpui::Hsla>>) -> Self {
        self.inactive_tab_border_color = color.into();
        self
    }

    pub fn with_tab_bar_colors(
        mut self,
        bg_color: impl Into<Option<gpui::Hsla>>,
        border_color: impl Into<Option<gpui::Hsla>>,
    ) -> Self {
        self.tab_bar_bg_color = bg_color.into();
        self.tab_bar_border_color = border_color.into();
        self
    }

    pub fn with_tab_item_colors(
        mut self,
        active_color: impl Into<Option<gpui::Hsla>>,
        hover_color: impl Into<Option<gpui::Hsla>>,
    ) -> Self {
        self.active_tab_bg_color = active_color.into();
        self.inactive_tab_hover_color = hover_color.into();
        self
    }

    pub fn with_tab_content_colors(
        mut self,
        text_color: impl Into<Option<gpui::Hsla>>,
        close_button_color: impl Into<Option<gpui::Hsla>>,
    ) -> Self {
        self.tab_text_color = text_color.into();
        self.tab_close_button_color = close_button_color.into();
        self
    }

    pub fn with_left_padding(mut self, padding: gpui::Pixels) -> Self {
        self.left_padding = Some(padding);
        self
    }

    pub fn with_top_padding(mut self, padding: gpui::Pixels) -> Self {
        self.top_padding = Some(padding);
        self
    }

    pub fn with_window_controls(mut self, show: bool) -> Self {
        self.show_window_controls = show;
        self
    }

    pub fn with_window_close_handler(
        mut self,
        handler: impl Fn(&mut Window, &mut App) + 'static + Send + Sync,
    ) -> Self {
        self.window_close_handler = Some(Arc::new(handler));
        self
    }

    pub fn set_tab_bar_trailing_view<V>(&mut self, view: V)
    where
        V: Into<AnyView>,
    {
        self.tab_bar_trailing_view = Some(view.into());
    }

    pub fn set_tab_list_header_action_label(&mut self, label: impl Into<SharedString>) {
        self.tab_list_header_action_label = Some(label.into());
    }

    /// Set a pinned tab that stays fixed before the scrollable tab list.
    /// The pinned tab is always visible and cannot be scrolled away.
    pub fn set_pinned_tab(&mut self, tab: TabItem, cx: &mut Context<Self>) {
        self.pinned_tab = Some(tab);
        self.pinned_tab_active = self.tabs.is_empty();
        self.reset_content_state_subscriptions(cx);
        cx.notify();
    }

    /// Returns whether the pinned tab is currently active.
    pub fn is_pinned_tab_active(&self) -> bool {
        self.pinned_tab_active
    }

    /// Returns whether a pinned tab exists.
    pub fn has_pinned_tab(&self) -> bool {
        self.pinned_tab.is_some()
    }

    /// Activate the pinned tab (deactivate regular tabs visually).
    pub fn activate_pinned_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pinned_tab.is_some() {
            // Deactivate the previously active regular tab
            if !self.pinned_tab_active {
                if let Some(old_tab) = self.tabs.get(self.active_index) {
                    old_tab.content().on_deactivate(window, cx);
                }
            }
            self.pinned_tab_active = true;
            if let Some(pinned) = &self.pinned_tab {
                pinned.content().focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        }
    }

    pub fn set_tab_bar_bg_color(
        &mut self,
        color: impl Into<Option<gpui::Hsla>>,
        cx: &mut Context<Self>,
    ) {
        self.tab_bar_bg_color = color.into();
        cx.notify();
    }

    pub fn set_tab_bar_border_color(
        &mut self,
        color: impl Into<Option<gpui::Hsla>>,
        cx: &mut Context<Self>,
    ) {
        self.tab_bar_border_color = color.into();
        cx.notify();
    }

    pub fn set_active_tab_bg_color(
        &mut self,
        color: impl Into<Option<gpui::Hsla>>,
        cx: &mut Context<Self>,
    ) {
        self.active_tab_bg_color = color.into();
        cx.notify();
    }

    pub fn set_inactive_tab_hover_color(
        &mut self,
        color: impl Into<Option<gpui::Hsla>>,
        cx: &mut Context<Self>,
    ) {
        self.inactive_tab_hover_color = color.into();
        cx.notify();
    }

    /// Add a new tab
    pub fn add_tab(&mut self, tab: TabItem, cx: &mut Context<Self>) {
        self.tabs.push(tab);
        self.reset_content_state_subscriptions(cx);
        cx.emit(TabContainerEvent::LayoutChanged);
        cx.notify();
    }

    /// Add a new tab and activate it
    pub fn add_and_activate_tab(&mut self, tab: TabItem, cx: &mut Context<Self>) {
        let id = tab.id().to_string();
        self.tabs.push(tab);
        self.reset_content_state_subscriptions(cx);
        self.active_index = self.tabs.len() - 1;
        self.pinned_tab_active = false;
        self.tab_bar_scroll_handle
            .scroll_to_item(self.tab_bar_scroll_target_index(self.tabs.len() - 1));
        cx.emit(TabContainerEvent::TabActivated {
            index: self.active_index,
            id,
        });
        cx.emit(TabContainerEvent::LayoutChanged);
        cx.notify();
    }

    /// Activate existing tab by ID, or create and activate if not exists (lazy loading)
    pub fn activate_or_add_tab_lazy<F>(
        &mut self,
        tab_id: impl Into<String>,
        create_fn: F,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(&mut Window, &mut Context<Self>) -> TabItem,
    {
        let tab_id = tab_id.into();

        if let Some(index) = self.tabs.iter().position(|t| t.id() == tab_id) {
            // 激活现有 tab，复用 set_active_index 逻辑
            self.set_active_index(index, window, cx);
        } else {
            // 创建新 tab 并激活
            let tab = create_fn(window, cx);
            self.add_and_activate_tab_with_focus(tab, window, cx);
        }
    }

    /// Add a new tab, activate it, and focus its content
    pub fn add_and_activate_tab_with_focus(
        &mut self,
        tab: TabItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = tab.id().to_string();
        let focus_handle = tab.content.focus_handle(cx);
        self.tabs.push(tab);
        self.reset_content_state_subscriptions(cx);
        self.active_index = self.tabs.len() - 1;
        self.pinned_tab_active = false;
        self.tab_bar_scroll_handle
            .scroll_to_item(self.tab_bar_scroll_target_index(self.tabs.len() - 1));

        // 激活新 tab 的 content
        if let Some(new_tab) = self.tabs.get(self.active_index) {
            new_tab.content().on_activate(window, cx);
        }

        // 让 content 获取焦点
        focus_handle.focus(window, cx);

        cx.emit(TabContainerEvent::TabActivated {
            index: self.active_index,
            id,
        });
        cx.emit(TabContainerEvent::LayoutChanged);
        cx.notify();
    }

    /// Close a tab by index
    pub fn close_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if index >= self.tabs.len() || !self.tabs[index].content().closeable(cx) {
            return Task::ready(false);
        }

        let tab_id = self.tabs[index].id();

        if self.closing_tabs.contains(&tab_id) {
            return Task::ready(false);
        }

        self.closing_tabs.insert(tab_id.clone());

        let tab_id_string = tab_id.to_string();
        let content = self.tabs[index].content().clone();
        let entity = cx.entity();

        let close_task = content.try_close(&tab_id_string, window, cx);

        cx.spawn(async move |_handle, cx| {
            // 超时保护：30 秒后若 try_close 仍未返回，强制移除 closing_tabs 标记。
            // 这里运行在 GPUI 的异步上下文中，不能直接依赖 Tokio reactor。
            let timeout_task = cx.background_executor().timer(std::time::Duration::from_secs(30));
            futures::pin_mut!(close_task);
            futures::pin_mut!(timeout_task);

            let can_close = match select(close_task, timeout_task).await {
                Either::Left((result, _)) => result,
                Either::Right(((), _)) => {
                    tracing::warn!("close_tab: try_close timeout for tab '{}', forcing removal from closing_tabs", tab_id_string);
                    let _ = entity.update(cx, |this, _cx| {
                        this.closing_tabs.remove(&tab_id);
                    });
                    return false;
                }
            };
            if can_close {
                let _ = entity.update(cx, |this, cx| {
                    this.do_remove_tab_by_id(&tab_id_string, cx);
                });
            } else {
                let _ = entity.update(cx, |this, _cx| {
                    this.closing_tabs.remove(&tab_id);
                });
            }
            can_close
        })
    }

    fn do_remove_tab_by_id(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| t.id() == tab_id) {
            let removed_tab_id = self.tabs[index].id();
            self.tabs.remove(index);
            self.closing_tabs.remove(&removed_tab_id);
            self.reset_content_state_subscriptions(cx);

            if self.tabs.is_empty() {
                // All regular tabs closed, activate pinned tab if present
                self.active_index = 0;
                if self.pinned_tab.is_some() {
                    self.pinned_tab_active = true;
                }
            } else if index < self.active_index {
                self.active_index -= 1;
            } else if index == self.active_index {
                if self.active_index >= self.tabs.len() {
                    self.active_index = self.tabs.len() - 1;
                }
            }

            cx.emit(TabContainerEvent::TabClosed {
                id: tab_id.to_string(),
            });
            cx.emit(TabContainerEvent::LayoutChanged);
            cx.notify();
        }
    }

    /// Close all tabs except the one at the given index
    pub fn close_other_tabs(
        &mut self,
        keep_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if keep_index >= self.tabs.len() {
            return Task::ready(true);
        }

        let keep_id = self.tabs[keep_index].id().to_string();
        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.id() != keep_id && t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_id = cx.active_window();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close =
                    cx.update_window(window_id.expect("No active window"), |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(index) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                this.set_active_index(index, window, cx);
                                let content = this.tabs[index].content().clone();
                                Some(content.try_close(&tab_id, window, cx))
                            } else {
                                None
                            }
                        })
                    });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    /// Close all tabs
    pub fn close_all_tabs(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_id = cx.active_window();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close =
                    cx.update_window(window_id.expect("No active window"), |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(index) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                this.set_active_index(index, window, cx);
                                let content = this.tabs[index].content().clone();
                                Some(content.try_close(&tab_id, window, cx))
                            } else {
                                None
                            }
                        })
                    });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    pub fn force_close_all_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_handle = window.window_handle();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close = cx.update_window(window_handle, |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        if let Some(index) = this.tabs.iter().position(|t| t.id() == tab_id) {
                            this.set_active_index(index, window, cx);
                            let content = this.tabs[index].content().clone();
                            Some(content.force_close(&tab_id, window, cx))
                        } else {
                            None
                        }
                    })
                });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    /// Close all tabs to the left of the given index
    pub fn close_tabs_to_left(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if index == 0 || index >= self.tabs.len() {
            return Task::ready(true);
        }

        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .take(index)
            .filter(|t| t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_id = cx.active_window();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close =
                    cx.update_window(window_id.expect("No active window"), |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(idx) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                this.set_active_index(idx, window, cx);
                                let content = this.tabs[idx].content().clone();
                                Some(content.try_close(&tab_id, window, cx))
                            } else {
                                None
                            }
                        })
                    });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    /// Close all tabs to the right of the given index
    pub fn close_tabs_to_right(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if index >= self.tabs.len() - 1 {
            return Task::ready(true);
        }

        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .skip(index + 1)
            .filter(|t| t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_id = cx.active_window();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close =
                    cx.update_window(window_id.expect("No active window"), |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(idx) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                this.set_active_index(idx, window, cx);
                                let content = this.tabs[idx].content().clone();
                                Some(content.try_close(&tab_id, window, cx))
                            } else {
                                None
                            }
                        })
                    });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    /// Close a tab by ID
    pub fn close_tab_by_id(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if let Some(index) = self.tabs.iter().position(|t| t.id() == id) {
            self.close_tab(index, window, cx)
        } else {
            Task::ready(false)
        }
    }

    /// Close all tabs from a specific source
    pub fn close_tabs_by_tab_from(
        &mut self,
        tab_from: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let tab_ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.from() == tab_from && t.content().closeable(cx))
            .map(|t| t.id().to_string())
            .collect();

        if tab_ids.is_empty() {
            return Task::ready(true);
        }

        let entity = cx.entity();
        let window_id = cx.active_window();

        cx.spawn(async move |_handle, cx| {
            for tab_id in tab_ids {
                let should_close =
                    cx.update_window(window_id.expect("No active window"), |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(index) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                this.set_active_index(index, window, cx);
                                let content = this.tabs[index].content().clone();
                                Some(content.try_close(&tab_id, window, cx))
                            } else {
                                None
                            }
                        })
                    });

                match should_close {
                    Ok(Some(task)) => {
                        let can_close = task.await;
                        if !can_close {
                            return false;
                        }
                        let _ = entity.update(cx, |this, cx| {
                            this.do_remove_tab_by_id(&tab_id, cx);
                        });
                    }
                    Ok(None) => continue,
                    Err(_) => return false,
                }
            }
            true
        })
    }

    /// Force close a tab by ID, skipping try_close
    pub fn force_close_tab_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        self.do_remove_tab_by_id(id, cx);
    }

    /// Set the active tab by index
    pub fn set_active_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() && (index != self.active_index || self.pinned_tab_active) {
            if self.pinned_tab_active {
                // Deactivate pinned tab
                if let Some(pinned) = &self.pinned_tab {
                    pinned.content().on_deactivate(window, cx);
                }
                self.pinned_tab_active = false;
            } else if let Some(old_tab) = self.tabs.get(self.active_index) {
                old_tab.content().on_deactivate(window, cx);
            }

            self.tab_bar_scroll_handle
                .scroll_to_item(self.tab_bar_scroll_target_index(index));
            self.active_index = index;

            let tab_id = if let Some(new_tab) = self.tabs.get(self.active_index) {
                new_tab.content().on_activate(window, cx);
                new_tab.content().focus_handle(cx).focus(window, cx);
                new_tab.id().to_string()
            } else {
                String::new()
            };

            cx.emit(TabContainerEvent::TabActivated { index, id: tab_id });
            cx.emit(TabContainerEvent::LayoutChanged);
            cx.notify();
        }
    }

    /// Set the active tab by ID
    pub fn set_active_by_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| t.id() == id) {
            self.set_active_index(index, window, cx);
        }
    }

    /// Get the active tab
    pub fn active_tab(&self) -> Option<&TabItem> {
        self.tabs.get(self.active_index)
    }

    pub fn current_title(&self, cx: &App) -> Option<SharedString> {
        if self.pinned_tab_active {
            self.pinned_tab.as_ref().map(|tab| tab.content().title(cx))
        } else {
            self.active_tab().map(|tab| tab.content().title(cx))
        }
    }

    pub fn current_status_summary(&self, cx: &App) -> Option<SharedString> {
        let current_tab = if self.pinned_tab_active {
            self.pinned_tab.as_ref()
        } else {
            self.active_tab()
        }?;
        let current_title = current_tab.content().title(cx);
        let content_key = current_tab.content().content_key(cx);
        let summary = current_tab.content().status_summary(cx)?;

        let summary_text = summary.to_string();
        let normalized_summary = summary_text.trim();
        if normalized_summary.is_empty() {
            return None;
        }

        if should_suppress_duplicate_status_summary(content_key)
            && current_title.to_string().trim().eq(normalized_summary)
        {
            return None;
        }

        Some(normalized_summary.to_string().into())
    }

    pub fn current_status_summary_element(&self, cx: &App) -> Option<gpui::AnyElement> {
        let current_tab = if self.pinned_tab_active {
            self.pinned_tab.as_ref()
        } else {
            self.active_tab()
        }?;
        current_tab.content().status_summary_element(cx)
    }

    pub fn set_size(&mut self, size: Size, cx: &mut Context<Self>) {
        self.size = size;
        cx.notify();
    }

    pub fn set_show_menu(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_menu = show;
        cx.notify();
    }

    fn tab_bar_scroll_target_index(&self, tab_index: usize) -> usize {
        let is_last_tab = !self.tabs.is_empty() && tab_index + 1 == self.tabs.len();
        if is_last_tab && self.tab_bar_trailing_view.is_some() {
            self.tabs.len()
        } else {
            tab_index
        }
    }

    fn handle_tab_bar_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.tab_bar_scroll_handle.bounds();
        if !bounds.contains(&event.position) {
            return;
        }

        let mut delta = event.delta.pixel_delta(window.line_height());
        if delta.x != px(0.0) && delta.y != px(0.0) {
            if delta.x.abs() > delta.y.abs() {
                delta.y = px(0.0);
            } else {
                delta.x = px(0.0);
            }
        }

        let horizontal_delta = if delta.x != px(0.0) { delta.x } else { delta.y };
        if horizontal_delta == px(0.0) {
            return;
        }

        let old_offset = self.tab_bar_scroll_handle.offset();
        let mut offset = old_offset;
        offset.x += horizontal_delta;
        offset.x = offset
            .x
            .clamp(-self.tab_bar_scroll_handle.max_offset().width, px(0.0));

        if offset != old_offset {
            self.tab_bar_scroll_handle.set_offset(offset);
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub fn scroll_to_tab_bar_trailing_view(&mut self, cx: &mut Context<Self>) {
        if self.tab_bar_trailing_view.is_some() {
            self.tab_bar_scroll_handle.scroll_to_item(self.tabs.len());
            cx.notify();
        }
    }

    fn request_tab_bar_trailing_action(&mut self, cx: &mut Context<Self>) {
        if self.tab_bar_trailing_view.is_some() {
            cx.emit(TabContainerEvent::TabBarTrailingActionRequested);
        }
    }

    fn refresh_tab_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs_data: Vec<(usize, SharedString, Option<Icon>, bool)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(idx, tab)| {
                (
                    idx,
                    tab.content().title(cx),
                    tab.content().icon(cx),
                    tab.content().closeable(cx),
                )
            })
            .collect();
        let container = cx.entity();
        let header_action_label = self.tab_list_header_action_label.clone();

        if let Some(tab_list) = &self.tab_list {
            tab_list.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.tabs = tabs_data.clone();
                delegate.filtered_tabs = tabs_data;
                delegate.header_action_label = header_action_label.clone();
            });
        } else {
            self.tab_list = Some(cx.new(|cx| {
                ListState::new(
                    TabListDelegate {
                        container,
                        tabs: tabs_data.clone(),
                        filtered_tabs: tabs_data,
                        header_action_label: header_action_label.clone(),
                        selected_index: None,
                    },
                    window,
                    cx,
                )
                .searchable(true)
            }));
        }
    }

    fn set_tab_list_popover_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list_popover_open = open;
        if open {
            self.refresh_tab_list(window, cx);
            if let Some(tab_list) = &self.tab_list {
                tab_list.focus_handle(cx).focus(window, cx);
            }
        }
        cx.notify();
    }

    pub fn tabs(&self) -> &[TabItem] {
        &self.tabs
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn dump(&self, cx: &App) -> TabContainerState {
        let tabs = self
            .tabs
            .iter()
            .map(|tab| TabItemState {
                id: tab.id(),
                from: tab.from(),
                key: SharedString::from(tab.content().content_key(cx)),
                data: tab.content().dump(cx),
            })
            .collect();

        TabContainerState {
            version: Some(1),
            tabs,
            active_index: self.active_index,
            config: self.dump_config(),
        }
    }

    fn dump_config(&self) -> TabContainerConfig {
        TabContainerConfig {
            size: Some(self.size_to_string()),
            left_padding: self.left_padding.map(|p| f32::from(p)),
            top_padding: self.top_padding.map(|p| f32::from(p)),
        }
    }

    fn size_to_string(&self) -> String {
        match self.size {
            Size::XSmall => "xsmall".to_string(),
            Size::Small => "small".to_string(),
            Size::Medium => "medium".to_string(),
            Size::Large => "large".to_string(),
            Size::Size(pixels) => format!("{}px", f32::from(pixels)),
        }
    }

    fn parse_size(s: &str) -> Size {
        match s {
            "xsmall" => Size::XSmall,
            "small" => Size::Small,
            "medium" => Size::Medium,
            "large" => Size::Large,
            s if s.ends_with("px") => s
                .trim_end_matches("px")
                .parse::<f32>()
                .map(|v| Size::Size(px(v)))
                .unwrap_or(Size::Large),
            _ => Size::Large,
        }
    }

    pub fn load(
        &mut self,
        state: TabContainerState,
        registry: &TabContentRegistry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 先关闭 pinned tab 激活状态，避免清空 tabs 时触发首页渲染
        self.pinned_tab_active = false;
        self.tabs.clear();
        self.content_subscriptions.clear();

        for tab_state in &state.tabs {
            if let Some(content) = registry.build(tab_state, window, cx) {
                self.tabs.push(TabItem {
                    id: tab_state.id.clone(),
                    from: tab_state.from.clone(),
                    content,
                });
            }
        }

        self.active_index = if self.tabs.is_empty() {
            0 // Empty list: active_index is 0 by convention (active_tab() will return None)
        } else {
            state.active_index.min(self.tabs.len() - 1)
        };

        self.load_config(&state.config);
        self.reset_content_state_subscriptions(cx);

        // 激活恢复的标签页
        if !self.tabs.is_empty() {
            if let Some(tab) = self.tabs.get(self.active_index) {
                tab.content().on_activate(window, cx);
            }
        }
    }

    fn active_content_id(&self, cx: &App) -> Option<EntityId> {
        if self.pinned_tab_active {
            self.pinned_tab
                .as_ref()
                .map(|tab| tab.content().content_id(cx))
        } else {
            self.active_tab().map(|tab| tab.content().content_id(cx))
        }
    }

    fn reset_content_state_subscriptions(&mut self, cx: &mut Context<Self>) {
        self.content_subscriptions.clear();

        let mut contents: Vec<Arc<dyn TabContentView>> =
            self.tabs.iter().map(|tab| tab.content().clone()).collect();

        if let Some(pinned_tab) = &self.pinned_tab {
            contents.push(pinned_tab.content().clone());
        }

        for content in contents {
            let content_id = content.content_id(cx);
            self.content_subscriptions
                .insert(content_id, content.subscribe_state(cx));
        }
    }

    fn load_config(&mut self, config: &TabContainerConfig) {
        if let Some(size) = &config.size {
            self.size = Self::parse_size(size);
        }
        if let Some(left_padding) = config.left_padding {
            let default_padding = self.left_padding.map(f32::from).unwrap_or(left_padding);
            self.left_padding = Some(px(left_padding.max(default_padding)));
        }
        if let Some(top_padding) = config.top_padding {
            let default_padding = self.top_padding.map(f32::from).unwrap_or(top_padding);
            self.top_padding = Some(px(top_padding.max(default_padding)));
        }
    }

    pub fn move_tab(&mut self, from_index: usize, to_index: usize, cx: &mut Context<Self>) {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() || from_index == to_index {
            return;
        }

        let tab = self.tabs.remove(from_index);
        self.tabs.insert(to_index, tab);

        if self.active_index == from_index {
            self.active_index = to_index;
        } else {
            match (
                from_index.cmp(&self.active_index),
                to_index.cmp(&self.active_index),
            ) {
                (Ordering::Less, Ordering::Greater | Ordering::Equal) => {
                    self.active_index -= 1;
                }
                (Ordering::Greater, Ordering::Less | Ordering::Equal) => {
                    self.active_index += 1;
                }
                _ => {}
            }
        }

        cx.emit(TabContainerEvent::LayoutChanged);
        cx.notify();
    }

    fn get_tab_width(&self, tab: &TabItem, window: &Window, cx: &App) -> gpui::Pixels {
        let size = tab.content().width_size(cx);
        // Size::Size(pixels) 直接返回，不参与比较
        if let Some(Size::Size(pixels)) = size {
            return pixels;
        }

        let title = tab.content().title(cx);
        let theme = cx.theme();
        let text_system = window.text_system();
        let font = gpui::font(theme.font_family.clone());
        let font_size = tab_title_measure_font_size(
            theme.font_size,
            cfg!(target_os = "macos"),
            cfg!(target_os = "windows"),
        );

        let text_run = TextRun {
            len: title.len(),
            font: font.clone(),
            color: theme.foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // 用整段 shaping 测量，避免逐字符 advance 在大字号下累积误差。
        let text_width = text_system
            .shape_text(title.clone(), font_size, &[text_run], None, None)
            .map(|lines| {
                lines
                    .iter()
                    .fold(px(0.0), |width, line| width.max(line.size(font_size).width))
            })
            .unwrap_or_else(|_| {
                let font_id = text_system.resolve_font(&font);
                title
                    .chars()
                    .map(|ch| {
                        text_system
                            .advance(font_id, font_size, ch)
                            .map(|advance| advance.width)
                            .unwrap_or_else(|_| {
                                if ch.is_ascii() {
                                    font_size * 0.5_f32
                                } else {
                                    font_size
                                }
                            })
                    })
                    .sum()
            });

        let has_icon = tab.content().icon(cx).is_some();
        let has_pending_indicator = tab.content().pending_change_level(cx).is_some();
        let closeable = tab.content().closeable(cx);
        let extras = tab_chrome_width(has_icon, has_pending_indicator, closeable);

        let calculated = text_width + extras;
        let max_width = self.size_to_pixels(size.unwrap_or(self.size));
        calculated.min(max_width)
    }

    fn size_to_pixels(&self, size: Size) -> gpui::Pixels {
        match size {
            Size::Size(pixels) => pixels,
            Size::XSmall => px(60.0),
            Size::Small => px(100.0),
            Size::Medium => px(140.0),
            Size::Large => px(180.0),
        }
    }

    pub fn render_tab_content(&self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let active_view = if self.pinned_tab_active {
            self.pinned_tab.as_ref().map(|tab| tab.content().view())
        } else {
            self.active_tab().map(|tab| tab.content().view())
        };

        div()
            .flex_1()
            .min_w_0()
            .w_full()
            .overflow_hidden()
            .when_some(active_view, |el, view| el.child(view))
    }

    pub fn render_tab_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let is_linux = cfg!(target_os = "linux");
        let is_macos = cfg!(target_os = "macos");
        let is_windows = cfg!(target_os = "windows");
        let is_client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let show_window_controls = self.show_window_controls;

        let theme = cx.theme();
        let is_dark_theme = theme.is_dark();
        let blur_enabled = theme.window_blur_enabled;
        let backdrop_opacity = theme.backdrop_opacity;
        let bg_color = resolve_tab_bar_color(
            self.tab_bar_bg_color,
            theme.tab_bar,
            blur_enabled,
            backdrop_opacity,
        );
        let border_color = self.tab_bar_border_color.unwrap_or(theme.border);
        let active_tab_color = self.active_tab_bg_color.unwrap_or_else(|| {
            layered_level_surface_color(
                theme.tab_active,
                blur_enabled,
                backdrop_opacity,
                2,
                WindowsSurfaceLayer::ContentBase,
            )
        });
        let inactive_tab_color = resolve_inactive_tab_color(
            self.inactive_tab_bg_color,
            self.tab_bar_bg_color,
            theme.tab,
            bg_color,
            backdrop_opacity,
            is_dark_theme,
            blur_enabled,
            backdrop_opacity,
        );
        let hover_tab_color = self.inactive_tab_hover_color.unwrap_or(theme.tab_hover);
        // 非激活标签边框色：基于 inactive tab 与主题边框共同生成，确保有辨识度。
        let inactive_tab_border = self.inactive_tab_border_color.unwrap_or_else(|| {
            default_inactive_tab_border_color(inactive_tab_color, border_color, is_dark_theme)
        });
        let text_color = self.tab_text_color.unwrap_or(theme.tab_foreground);
        let active_text_color = theme.tab_active_foreground;
        let close_btn_color = self
            .tab_close_button_color
            .unwrap_or(theme.muted_foreground);
        let drag_border_color = theme.drag_border;
        let first_tab_corner_radius = theme.radius;
        // Pending change indicator colors
        let indicator_red = theme.red;
        let indicator_yellow = theme.yellow;
        let indicator_green = theme.green;
        let indicator_default = theme.muted_foreground;
        let active_index = self.active_index;
        let left_padding = self.left_padding.unwrap_or(px(0.0));

        let tab_list = self.tab_list.clone();
        let tab_list_popover_open = self.list_popover_open;

        // 窗口拖动状态管理（仅在 Windows/Linux 上需要，且启用窗口控件时）
        let show_custom_window_controls =
            show_window_controls && should_render_custom_window_controls(window);
        let manual_window_move = uses_manual_window_move(show_window_controls, is_windows);
        let show_windows_drag_spacer =
            should_render_windows_drag_spacer(show_window_controls, is_windows);
        let show_inline_drag_spacer =
            should_render_inline_drag_spacer(show_window_controls, is_windows, is_linux);
        // 所有非 macOS 平台都启用 tab 拖拽重排。Windows 上的窗口拖动由
        // WindowControlArea::Drag 独立热区（tab-bar-drag-spacer）处理，
        // 与 tab 自身的 on_drag 互不影响。
        // macOS 也启用 tab 拖拽，窗口拖动冲突由 should_block_tab_mouse_for_window_move 保护。
        let drag_plan = build_tab_bar_drag_plan(
            show_window_controls,
            self.pinned_tab.is_some(),
            !self.tabs.is_empty(),
        );

        // 非 Windows 平台使用状态管理窗口拖动；Windows 依赖 WindowControlArea 命中测试。
        let drag_state = window.use_state(cx, |_, _| TabBarDragState { should_move: false });
        let interaction_state = window.use_state(cx, |_, _| TabBarInteractionState {
            tab_click_active: false,
        });

        h_flex()
            .id("tab-bar")
            .w_full()
            .h(TAB_BAR_HEIGHT)
            .bg(bg_color)
            .when(true, |this| {
                this.rounded_tl(cx.theme().radius_lg)
                    .rounded_tr(cx.theme().radius_lg)
                // .pl(px(4.0))
                // .pr(px(4.0))
            })
            .overflow_hidden()
            .items_center()
            .border_b_1()
            .border_color(border_color)
            .on_scroll_wheel(cx.listener(Self::handle_tab_bar_scroll_wheel))
            .when(show_windows_drag_spacer, |this| {
                this.window_control_area(WindowControlArea::Drag)
            })
            // macOS 双击标签栏触发系统偏好设置的标题栏双击行为（zoom/minimize）
            .when(is_macos, |this| {
                this.on_double_click(|_, window, _| window.handle_titlebar_double_click())
            })
            // 窗口拖动支持：仅在非 macOS 且启用窗口控件时生效
            .when(manual_window_move, |this| {
                this.when(is_linux, |this| {
                    this.on_double_click(|_, window, _| window.zoom_window())
                })
                .on_mouse_down_out(window.listener_for(&drag_state, |state, _, _, _| {
                    state.should_move = false;
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&drag_state, |state, _, _, _| {
                        state.should_move = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&drag_state, |state, _, _, _| {
                        state.should_move = false;
                    }),
                )
                .on_mouse_move(window.listener_for(
                    &drag_state,
                    |state, _, window, _| {
                        if state.should_move {
                            state.should_move = false;
                            window.start_window_move();
                        }
                    },
                ))
            })
            .when(is_macos, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .h_full()
                        .w(left_padding)
                        .when_some(self.top_padding, |div, padding| div.pt(padding)),
                )
            })
            // Pinned tab (fixed, not scrollable)
            .when_some(self.pinned_tab.as_ref(), |this, pinned| {
                let pinned_title = pinned.content().title(cx);
                let pinned_icon = pinned.content().icon(cx);
                let is_pinned_active = self.pinned_tab_active;
                let view_for_pinned = view.clone();
                let top_padding = self.top_padding;

                this.child(
                    div()
                        .id("pinned-tab")
                        .flex()
                        .occlude()
                        .on_scroll_wheel(cx.listener(Self::handle_tab_bar_scroll_wheel))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .items_center()
                        .gap_2()
                        .h(px(32.0))
                        .cursor_pointer()
                        .when(!is_macos, |el| el.ml(left_padding))
                        .when_some(top_padding, |el, padding| el.mt(padding))
                        .rounded(px(6.0))
                        .when(!is_macos, |el| el.rounded_tl(first_tab_corner_radius))
                        .when(is_pinned_active, |el| el.bg(active_tab_color).px(px(8.0)))
                        .when(!is_pinned_active, |el| {
                            el.hover(move |style| style.bg(hover_tab_color))
                                .bg(inactive_tab_color)
                                .border_1()
                                .border_color(inactive_tab_border)
                                .px(px(8.0))
                        })
                        .when(drag_plan.enable_single_pinned_tab_drag, |el| {
                            el.window_control_area(WindowControlArea::Drag)
                                .on_mouse_down_out(window.listener_for(
                                    &drag_state,
                                    |state, _, _, _| {
                                        state.should_move = false;
                                    },
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    window.listener_for(&drag_state, |state, _, _, _| {
                                        state.should_move = true;
                                    }),
                                )
                                .on_mouse_up(
                                    MouseButton::Left,
                                    window.listener_for(&drag_state, |state, _, _, _| {
                                        state.should_move = false;
                                    }),
                                )
                                .on_mouse_move(window.listener_for(
                                    &drag_state,
                                    |state, _, window, _| {
                                        if state.should_move {
                                            state.should_move = false;
                                            window.start_window_move();
                                        }
                                    },
                                ))
                        })
                        .when(!drag_plan.enable_single_pinned_tab_drag, |el| {
                            el.cursor_pointer()
                                // pinned tab 在存在普通 tab 时只是一个普通可点击页签，
                                // 需要阻止事件冒泡到标题栏拖动区域。
                                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                })
                                .on_mouse_move(|_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                })
                                .on_click(move |_, window, cx| {
                                    view_for_pinned.update(cx, |this, cx| {
                                        this.activate_pinned_tab(window, cx);
                                    });
                                })
                        })
                        .when_some(pinned_icon, |el, icon| {
                            el.child(div().flex_shrink_0().flex().items_center().child(icon))
                        })
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_sm()
                                .text_color(if is_pinned_active {
                                    active_text_color
                                } else {
                                    text_color
                                })
                                .text_ellipsis()
                                .child(pinned_title.to_string()),
                        ),
                )
                // 分隔线已移除，改用 ml(px(2.0)) 间隔
            })
            .child(
                h_flex()
                    .id("tabs-scroll-region")
                    .flex_1()
                    .min_w(px(0.0))
                    .relative()
                    .ml(px(2.0))
                    // `overflow_hidden()` + `track_scroll()` 保留横向滚动能力，
                    // 同时隐藏系统滚动条；滚轮事件只作用于 tab 列表本身。
                    // Windows 使用 window_control_area(WindowControlArea::Drag) 提供原生拖动。
                    .when(manual_window_move, |this| {
                        this.on_mouse_down_out(window.listener_for(
                            &drag_state,
                            |state, _, _, _| {
                                state.should_move = false;
                            },
                        ))
                        .on_mouse_down(MouseButton::Left, {
                            let drag_state = drag_state.clone();
                            let interaction_state = interaction_state.clone();
                            move |_: &gpui::MouseDownEvent, _, cx| {
                                // 如果当前有活跃的 tab 点击（tab 的 on_mouse_down 先执行），则不设置窗口拖动标志。
                                if !interaction_state.read(cx).tab_click_active {
                                    drag_state.update(cx, |state, _| {
                                        state.should_move = true;
                                    });
                                }
                            }
                        })
                        .on_mouse_up(MouseButton::Left, {
                            let drag_state = drag_state.clone();
                            let interaction_state = interaction_state.clone();
                            window.listener_for(&drag_state, move |state, _, _, cx| {
                                state.should_move = false;
                                interaction_state.update(cx, |s, _| {
                                    s.tab_click_active = false;
                                });
                            })
                        })
                        .on_mouse_move(window.listener_for(&drag_state, |state, _, window, _| {
                            if state.should_move {
                                state.should_move = false;
                                window.start_window_move();
                            }
                        }))
                    })
                    // 仅在启用窗口控件且无 manual_window_move 时使用原生拖动区域
                    .when(
                        !manual_window_move && drag_plan.enable_scroll_area_drag,
                        |this| this.window_control_area(WindowControlArea::Drag),
                    )
                    .overflow_hidden()
                    .overflow_x_scroll()
                    .when(!is_macos && self.pinned_tab.is_none(), |this| {
                        this.pl(left_padding)
                    })
                    .when_some(self.top_padding, |div, padding| div.pt(padding))
                    .pr_2()
                    .gap(px(2.))
                    .track_scroll(&self.tab_bar_scroll_handle)
                    .children(self.tabs.iter().enumerate().map(|(idx, tab)| {
                        let title = tab.content().title(cx);
                        let icon = tab.content().icon(cx);
                        let closeable = tab.content().closeable(cx);
                        let is_active =
                            is_regular_tab_active(idx, active_index, self.pinned_tab_active);
                        let view_clone = view.clone();
                        let title_clone = title.clone();
                        let tab_width = self.get_tab_width(tab, window, cx);
                        let interaction_state = interaction_state.clone();
                        let pending_change_level = tab.content().pending_change_level(cx);
                        // 根据 pending_change_level 确定指示器颜色
                        let indicator_color = match pending_change_level {
                            Some(PendingChangeLevel::Delete) => indicator_red,
                            Some(PendingChangeLevel::Modify) => indicator_yellow,
                            Some(PendingChangeLevel::Insert) => indicator_green,
                            None => indicator_default,
                        };
                        let has_pending_indicator = pending_change_level.is_some();

                        div()
                            .id(idx)
                            .flex()
                            // Windows 需要彻底遮住父级标题栏拖动区，避免 tab 命中
                            // 被系统当成 WindowControlArea::Drag。
                            // 其它平台仍允许滚轮穿透到底层滚动容器。
                            .when(is_windows, |el| el.occlude())
                            .when(!is_windows, |el| el.block_mouse_except_scroll())
                            .on_scroll_wheel(cx.listener(Self::handle_tab_bar_scroll_wheel))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .items_center()
                            .gap_2()
                            .h(px(32.0))
                            .cursor_pointer()
                            .text_ellipsis()
                            .w(tab_width)
                            .rounded(px(6.0))
                            .when(!is_macos && self.pinned_tab.is_none() && idx == 0, |el| {
                                el.rounded_tl(first_tab_corner_radius)
                            })
                            .when(is_active, |el| el.bg(active_tab_color).px(px(8.0)))
                            .when(!is_active, |el| {
                                el.hover(move |style| style.bg(hover_tab_color))
                                    .bg(inactive_tab_color)
                                    .border_1()
                                    .border_color(inactive_tab_border)
                                    .px(px(8.0))
                            })
                            // 普通 tab 不应把拖动/按下事件冒泡为窗口拖动。
                            // 设置 tab_click_active 标志，防止 scroll region 的窗口拖动干扰。
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_evt, window: &mut Window, cx| {
                                    interaction_state.update(cx, |state, _| {
                                        state.tab_click_active = true;
                                    });
                                    window.prevent_default();
                                    cx.stop_propagation();
                                },
                            )
                            .on_mouse_move(move |_evt, window: &mut Window, _cx| {
                                window.prevent_default();
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_active_index(idx, window, cx);
                            }))
                            .cursor_grab()
                            .drag_threshold(TAB_REORDER_DRAG_THRESHOLD)
                            .on_drag(DragTab::new(idx, title.clone()), |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| drag.clone())
                            })
                            // on_drop 和 drag_over 在所有 tab 上注册，接收来自其他 tab 的 drop 事件
                            .drag_over::<DragTab>(move |el, _, _, _cx| {
                                el.border_l_2().border_color(drag_border_color)
                            })
                            .on_drop(cx.listener(move |this, drag: &DragTab, window, cx| {
                                let from_idx = drag.tab_index;
                                let to_idx = idx;
                                if from_idx != to_idx {
                                    this.move_tab(from_idx, to_idx, cx);
                                }
                                this.set_active_index(to_idx, window, cx);
                            }))
                            .when_some(icon, |el, icon| {
                                el.child(div().flex_shrink_0().flex().items_center().child(icon))
                            })
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap(TAB_COMPACT_ITEM_GAP)
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_sm()
                                            .text_color(if is_active {
                                                active_text_color
                                            } else {
                                                text_color
                                            })
                                            .text_ellipsis()
                                            .child(title_clone.to_string()),
                                    )
                                    .when(has_pending_indicator, |group| {
                                        group.child(
                                            div()
                                                .flex_shrink_0()
                                                .w(px(8.0))
                                                .h(px(8.0))
                                                .rounded_full()
                                                .bg(indicator_color),
                                        )
                                    }),
                            )
                            .when(closeable, |group| {
                                let view_clone = view_clone.clone();
                                let close_button_style = ButtonCustomVariant::new(cx)
                                    .color(cx.theme().transparent)
                                    .foreground(close_btn_color)
                                    .border(cx.theme().transparent)
                                    .hover(cx.theme().warning)
                                    .active(cx.theme().warning_active);
                                group.child(
                                    Button::new(SharedString::from(format!("tab-close-btn-{idx}")))
                                        .icon(IconName::Close)
                                        .custom(close_button_style)
                                        .compact()
                                        .tab_stop(false)
                                        .occlude()
                                        .flex_shrink_0()
                                        .w(px(16.0))
                                        .h(px(16.0))
                                        .min_w(px(16.0))
                                        .p_0()
                                        .rounded(px(2.0))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            move |_event, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                            },
                                        )
                                        .on_click(move |_, window, cx| {
                                            cx.stop_propagation();
                                            view_clone.update(cx, |this, cx| {
                                                this.close_tab(idx, window, cx).detach();
                                            });
                                        }),
                                )
                            })
                            .context_menu(move |menu, window, cx| {
                                let view_for_menu = view_clone.clone();
                                let (tab_count, closeable, is_ssh_tab, tab_id) = {
                                    let view = view_for_menu.read(cx);
                                    let tab = view.tabs.get(idx);
                                    (
                                        view.tabs.len(),
                                        tab.map(|tab| tab.content().closeable(cx)).unwrap_or(false),
                                        tab.map(|tab| tab.from() == "ssh").unwrap_or(false),
                                        tab.map(|tab| tab.id().to_string()),
                                    )
                                };
                                let has_tabs_left = idx > 0;
                                let has_tabs_right = idx < tab_count - 1;
                                let menu =
                                    if is_ssh_tab {
                                        if let Some(tab_id) = tab_id {
                                            menu.item(
                                                PopupMenuItem::new(
                                                    t!("TabContainer.menu_open_sftp").to_string(),
                                                )
                                                .on_click(window.listener_for(
                                                    &view_for_menu,
                                                    move |_this, _, _window, cx| {
                                                        cx.emit(
                                                            TabContainerEvent::OpenSftpRequested {
                                                                tab_id: tab_id.clone(),
                                                            },
                                                        );
                                                    },
                                                )),
                                            )
                                            .item(PopupMenuItem::separator())
                                        } else {
                                            menu
                                        }
                                    } else {
                                        menu
                                    };

                                menu.item(
                                    PopupMenuItem::new(t!("TabContainer.menu_close").to_string())
                                        .disabled(!closeable)
                                        .on_click(window.listener_for(
                                            &view_for_menu,
                                            move |this, _, window, cx| {
                                                this.close_tab(idx, window, cx).detach();
                                            },
                                        )),
                                )
                                .item(
                                    PopupMenuItem::new(
                                        t!("TabContainer.menu_close_all").to_string(),
                                    )
                                    .on_click(
                                        window.listener_for(
                                            &view_for_menu,
                                            move |this, _, window, cx| {
                                                this.close_all_tabs(window, cx).detach();
                                            },
                                        ),
                                    ),
                                )
                                .item(
                                    PopupMenuItem::new(
                                        t!("TabContainer.menu_close_others").to_string(),
                                    )
                                    .disabled(tab_count <= 1)
                                    .on_click(
                                        window.listener_for(
                                            &view_for_menu,
                                            move |this, _, window, cx| {
                                                this.close_other_tabs(idx, window, cx).detach();
                                            },
                                        ),
                                    ),
                                )
                                .item(
                                    PopupMenuItem::new(
                                        t!("TabContainer.menu_close_tabs_to_left").to_string(),
                                    )
                                    .disabled(!has_tabs_left)
                                    .on_click(
                                        window.listener_for(
                                            &view_for_menu,
                                            move |this, _, window, cx| {
                                                this.close_tabs_to_left(idx, window, cx).detach();
                                            },
                                        ),
                                    ),
                                )
                                .item(
                                    PopupMenuItem::new(
                                        t!("TabContainer.menu_close_tabs_to_right").to_string(),
                                    )
                                    .disabled(!has_tabs_right)
                                    .on_click(
                                        window.listener_for(
                                            &view_for_menu,
                                            move |this, _, window, cx| {
                                                this.close_tabs_to_right(idx, window, cx).detach();
                                            },
                                        ),
                                    ),
                                )
                            })
                    }))
                    .when_some(self.tab_bar_trailing_view.clone(), |this, view| {
                        this.child(
                            h_flex()
                                .id("tab-bar-trailing-controls")
                                .flex_shrink_0()
                                .items_center()
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                })
                                .child(view),
                        )
                    })
                    .when(show_inline_drag_spacer, |this| {
                        this.child(
                            div()
                                .id("tab-bar-inline-drag-spacer")
                                .flex_grow()
                                .min_w(WINDOWS_TAB_BAR_DRAG_SPACER_WIDTH)
                                .h_full()
                                .occlude()
                                .when(is_windows, |el| {
                                    el.window_control_area(WindowControlArea::Drag)
                                })
                                .when(manual_window_move, |el| {
                                    el.on_mouse_down_out(window.listener_for(
                                        &drag_state,
                                        |state, _, _, _| {
                                            state.should_move = false;
                                        },
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        window.listener_for(&drag_state, |state, _, _, _| {
                                            state.should_move = true;
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        window.listener_for(&drag_state, |state, _, _, _| {
                                            state.should_move = false;
                                        }),
                                    )
                                    .on_mouse_move(
                                        window.listener_for(&drag_state, |state, _, window, _| {
                                            if state.should_move {
                                                state.should_move = false;
                                                window.start_window_move();
                                            }
                                        }),
                                    )
                                }),
                        )
                    })
                    // Linux 客户端装饰模式下，右键显示窗口菜单。
                    // 这个覆盖层必须放在 tabs 之后，避免被 ScrollHandle
                    // 计入前置 child 索引，导致 scroll_to_item(index) 对错目标。
                    .when(
                        is_linux && is_client_decorated && show_window_controls,
                        |this| {
                            this.child(
                                div()
                                    .top_0()
                                    .left_0()
                                    .absolute()
                                    .size_full()
                                    .h_full()
                                    .on_mouse_down(MouseButton::Right, move |ev, window, _| {
                                        window.show_window_menu(ev.position)
                                    }),
                            )
                        },
                    ),
            )
            .child(
                Popover::new("tab-list-popover")
                    .mouse_button(MouseButton::Right)
                    .anchor(Corner::TopRight)
                    // .p_0()
                    .open(self.list_popover_open)
                    .on_open_change(cx.listener(move |this, open, window, cx| {
                        this.set_tab_list_popover_open(*open, window, cx);
                    }))
                    .when_some(tab_list.as_ref(), |popover, list| {
                        popover.track_focus(&list.focus_handle(cx))
                    })
                    .trigger(
                        Button::new("tab-dropdown-btn")
                            .icon(IconName::ChevronDown)
                            .cursor_pointer()
                            .ghost()
                            .compact()
                            .occlude()
                            .selected(tab_list_popover_open)
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click({
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        let next_open = !this.list_popover_open;
                                        this.set_tab_list_popover_open(next_open, window, cx);
                                    });
                                }
                            }),
                    )
                    .when_some(tab_list, |popover, list| {
                        popover.child(
                            List::new(&list)
                                .w(px(280.0))
                                .max_h(px(300.0))
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(cx.theme().radius),
                        )
                    }),
            )
            .when(show_custom_window_controls, |el| {
                el.child(self.render_window_controls(window, cx))
            })
    }

    fn render_window_controls(&self, window: &mut Window, cx: &App) -> impl IntoElement {
        let is_linux = cfg!(target_os = "linux");
        let is_windows = cfg!(target_os = "windows");
        let is_maximized = window.is_maximized();
        let should_round_control_buttons = is_linux && linux_prefers_system_window_controls();

        h_flex()
            .id("window-controls")
            .items_center()
            .flex_shrink_0()
            .h_full()
            .child(self.render_control_button(
                "minimize",
                IconName::WindowMinimize,
                WindowControlArea::Min,
                is_linux,
                is_windows,
                false,
                should_round_control_buttons,
                cx,
            ))
            .child(self.render_control_button(
                if is_maximized { "restore" } else { "maximize" },
                if is_maximized {
                    IconName::WindowRestore
                } else {
                    IconName::WindowMaximize
                },
                WindowControlArea::Max,
                is_linux,
                is_windows,
                false,
                should_round_control_buttons,
                cx,
            ))
            .child(self.render_control_button(
                "close",
                IconName::WindowClose,
                WindowControlArea::Close,
                is_linux,
                is_windows,
                true,
                should_round_control_buttons,
                cx,
            ))
    }

    fn render_control_button(
        &self,
        id: &'static str,
        icon: IconName,
        control_area: WindowControlArea,
        is_linux: bool,
        is_windows: bool,
        is_close: bool,
        round_self: bool,
        cx: &App,
    ) -> impl IntoElement {
        let window_close_handler = self.window_close_handler.clone();
        div()
            .id(id)
            .flex()
            .when(is_windows, |this| this.occlude())
            .w(WINDOW_CONTROL_BUTTON_SIZE)
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .text_color(gpui::white())
            .when(round_self, |this| {
                this.rounded(cx.theme().radius_lg).overflow_hidden()
            })
            .hover(move |style| {
                if round_self || is_close {
                    style.bg(gpui::rgb(0xe81123)).text_color(gpui::white())
                } else {
                    style.bg(gpui::rgb(0x3a3a3a)).text_color(gpui::white())
                }
            })
            .active(move |style| {
                if is_close {
                    style.bg(gpui::rgb(0xc50f1f)).text_color(gpui::white())
                } else {
                    style.bg(gpui::rgb(0x2a2a2a)).text_color(gpui::white())
                }
            })
            .when(is_windows, move |this| {
                // Windows 依赖系统原生标题栏控件行为：
                // 仅声明 control area，避免手动 on_click 干扰最大化/还原切换。
                this.window_control_area(control_area)
            })
            .when(is_linux, move |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match control_area {
                        WindowControlArea::Min => window.minimize_window(),
                        WindowControlArea::Max => window.zoom_window(),
                        WindowControlArea::Close => {
                            if let Some(handler) = window_close_handler.as_ref() {
                                handler(window, cx);
                            } else {
                                window.remove_window();
                            }
                        }
                        _ => {}
                    }
                })
            })
            .child(Icon::new(icon).with_size(Size::Small))
    }
}

impl Focusable for TabContainer {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.pinned_tab_active {
            if let Some(pinned) = &self.pinned_tab {
                return pinned.content().focus_handle(cx);
            }
        }
        if let Some(active_tab) = self.active_tab() {
            active_tab.content().focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }
}

impl Render for TabContainer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle(cx);

        div()
            .id("tab-container")
            .track_focus(&focus_handle)
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .child(self.render_tab_bar(window, cx))
                    .child(self.render_tab_content(window, cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TabBarDragPlan, build_tab_bar_drag_plan, default_inactive_tab_border_color,
        default_inactive_tab_color, inactive_tab_background_alpha, is_regular_tab_active,
        resolve_inactive_tab_color, resolve_tab_bar_color, should_render_windows_drag_spacer,
        should_suppress_duplicate_status_summary, uses_manual_window_move,
    };
    use gpui::hsla;

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-6);
    }

    #[test]
    fn windows_仅渲染独立拖窗热区() {
        assert!(should_render_windows_drag_spacer(true, true));
        assert!(!should_render_windows_drag_spacer(true, false));
        assert!(!should_render_windows_drag_spacer(false, true));
    }

    #[test]
    fn windows_与_linux_渲染内联拖窗热区() {
        assert!(should_render_inline_drag_spacer(true, true, false));
        assert!(should_render_inline_drag_spacer(true, false, true));
        assert!(!should_render_inline_drag_spacer(true, false, false));
        assert!(!should_render_inline_drag_spacer(false, true, true));
    }

    #[test]
    fn 非_windows_保留手动拖窗链路() {
        assert!(uses_manual_window_move(true, false));
        assert!(!uses_manual_window_move(true, true));
        assert!(!uses_manual_window_move(false, false));
    }

    #[test]
    fn build_tab_bar_drag_plan_enables_pinned_drag_for_single_home_tab() {
        let plan = build_tab_bar_drag_plan(true, true, false);

        assert_eq!(
            plan,
            TabBarDragPlan {
                enable_scroll_area_drag: true,
                enable_single_pinned_tab_drag: true,
            }
        );
    }

    #[test]
    fn build_tab_bar_drag_plan_keeps_pinned_drag_disabled_when_other_tabs_exist() {
        let plan = build_tab_bar_drag_plan(true, true, true);

        assert_eq!(
            plan,
            TabBarDragPlan {
                enable_scroll_area_drag: true,
                enable_single_pinned_tab_drag: false,
            }
        );
    }

    #[test]
    fn build_tab_bar_drag_plan_disables_all_drag_without_window_controls() {
        let plan = build_tab_bar_drag_plan(false, true, false);

        assert_eq!(
            plan,
            TabBarDragPlan {
                enable_scroll_area_drag: false,
                enable_single_pinned_tab_drag: false,
            }
        );
    }

    #[test]
    fn inactive_tab_alpha_整体仍透明时加_point_four() {
        assert_f32_close(inactive_tab_background_alpha(0.4, 0.7), 0.8);
        assert_f32_close(inactive_tab_background_alpha(0.55, 0.85), 0.95);
    }

    #[test]
    fn inactive_tab_alpha_整体不透明时加_point_two() {
        assert_f32_close(inactive_tab_background_alpha(0.4, 1.0), 0.6);
        assert_f32_close(inactive_tab_background_alpha(0.84, 1.0), 1.0);
    }

    #[test]
    fn 暗色主题inactive_tab比tab_bar更亮() {
        let tab_bar = hsla(0.0, 0.0, 0.16, 1.0);
        let inactive = default_inactive_tab_color(tab_bar, 0.84, true);

        assert!(inactive.l > tab_bar.l);
        assert_eq!(inactive.a, 1.0);
    }

    #[test]
    fn 亮色主题inactive_tab比tab_bar更暗() {
        let tab_bar = hsla(0.0, 0.0, 0.96, 1.0);
        let inactive = default_inactive_tab_color(tab_bar, 0.84, false);

        assert!(inactive.l < tab_bar.l);
        assert_eq!(inactive.a, 1.0);
    }

    #[test]
    fn 默认tab_bar直接使用主题tab_bar颜色() {
        let theme_tab_bar = hsla(0.63, 0.18, 0.12, 0.91);

        assert_eq!(resolve_tab_bar_color(None, theme_tab_bar), theme_tab_bar);
    }

    #[test]
    fn 默认inactive_tab直接使用主题tab颜色() {
        let theme_tab = hsla(0.63, 0.18, 0.18, 0.94);
        let resolved = resolve_inactive_tab_color(
            None,
            None,
            theme_tab,
            hsla(0.63, 0.18, 0.12, 0.91),
            0.84,
            true,
        );

        assert_eq!(resolved, theme_tab);
    }

    #[test]
    fn 显式覆盖tab_bar时inactive_tab仍按tab_bar推导() {
        let custom_tab_bar = hsla(0.0, 0.0, 0.16, 0.7);
        let expected = default_inactive_tab_color(custom_tab_bar, 0.84, true);
        let resolved = resolve_inactive_tab_color(
            None,
            Some(custom_tab_bar),
            hsla(0.0, 0.0, 0.22, 0.92),
            custom_tab_bar,
            0.84,
            true,
        );

        assert_eq!(resolved, expected);
    }

    #[test]
    fn inactive_tab边框保持比背景更有辨识度() {
        let dark_inactive = hsla(0.0, 0.0, 0.22, 1.0);
        let dark_border =
            default_inactive_tab_border_color(dark_inactive, hsla(0.0, 0.0, 0.25, 1.0), true);
        assert!(dark_border.l > dark_inactive.l);

        let light_inactive = hsla(0.0, 0.0, 0.90, 1.0);
        let light_border =
            default_inactive_tab_border_color(light_inactive, hsla(0.0, 0.0, 0.88, 1.0), false);
        assert!(light_border.l < light_inactive.l);
    }

    #[test]
    fn pinned_tab_active时普通tab不应同时处于激活态() {
        assert!(!is_regular_tab_active(2, 2, true));
        assert!(is_regular_tab_active(2, 2, false));
        assert!(!is_regular_tab_active(1, 2, false));
    }

    #[test]
    fn terminal_status_summary_不做标题去重() {
        assert!(!should_suppress_duplicate_status_summary("Terminal"));
    }

    #[test]
    fn 非_terminal_status_summary_仍做标题去重() {
        assert!(should_suppress_duplicate_status_summary("Database"));
    }
}
