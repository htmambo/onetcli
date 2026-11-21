//! Redis 键值视图

use std::ops::Range;

use crate::{
    GlobalRedisState, HashField, KeyInfo, KeyValueContent, KeyValueDetail, RedisKeyType, ZSetMember,
};
use gpui::{
    AnyElement, App, AppContext, AsyncApp, ClipboardItem, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Task, Window, div, prelude::FluentBuilder, px, relative,
    uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, IndexPath, Sizable, Size, WindowExt as _, WindowsSurfaceLayer,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    h_flex,
    highlighter::Language,
    input::{Input, InputEvent, InputState},
    layered_level_surface_color,
    radio::Radio,
    select::{Select, SelectEvent, SelectItem, SelectState},
    spinner::Spinner,
    v_flex,
};
use one_core::gpui_tokio::Tokio;
use one_core::tab_container::{TabContent, TabContentEvent};
use one_ui::{LargeTextEditor, create_large_text_editor_with_content};
use rust_i18n::t;

/// 键值视图事件
#[derive(Clone, Debug)]
pub enum KeyValueViewEvent {
    /// 值已更新
    ValueUpdated { key: String },
    /// 值已删除
    ValueDeleted { key: String },
}

/// 加载状态
#[derive(Clone, Debug, PartialEq)]
enum LoadState {
    Empty,
    Loading,
    Loaded,
    Error(String),
}

fn content_section_bg(cx: &App) -> gpui::Hsla {
    layered_level_surface_color(
        cx.theme().muted,
        cx.theme().window_blur_enabled,
        cx.theme().backdrop_opacity,
        1,
        WindowsSurfaceLayer::ContentSection,
    )
}

/// 查看格式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewFormat {
    #[default]
    Raw,
    Json,
    Hex,
    Binary,
}

impl ViewFormat {
    pub fn all() -> Vec<Self> {
        vec![
            ViewFormat::Raw,
            ViewFormat::Json,
            ViewFormat::Hex,
            ViewFormat::Binary,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ViewFormat::Raw => "Raw",
            ViewFormat::Json => "JSON",
            ViewFormat::Hex => "Hex",
            ViewFormat::Binary => "Binary",
        }
    }
}

impl SelectItem for ViewFormat {
    type Value = ViewFormat;

    fn title(&self) -> SharedString {
        self.display_name().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

fn large_text_preview_title(label: &str, context: Option<&str>) -> String {
    match context.map(str::trim).filter(|context| !context.is_empty()) {
        Some(context) => format!("{label} {context}"),
        None => label.to_string(),
    }
}

fn should_replace_set_member(old_member: &str, new_member: &str) -> bool {
    old_member != new_member
}

/// List 插入位置
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListInsertPosition {
    /// 头部插入 (LPUSH)
    #[default]
    Head,
    /// 尾部插入 (RPUSH)
    Tail,
}

/// 排序方向
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// ZSet 排序字段
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZSetSortBy {
    #[default]
    Score,
    Member,
}

/// 明细视图的筛选/排序结果缓存。
///
/// Redis 明细最多可达上千个成员，每帧重新筛选、排序并克隆会明显拖慢渲染。
/// 这里按（内容版本、筛选条件、排序条件）缓存结果，条件不变时直接复用。
#[derive(Default)]
struct FilteredContent {
    /// 缓存对应的内容版本
    content_version: u64,
    /// 缓存对应的筛选文本
    filter_text: String,
    /// 缓存对应的精确匹配开关
    filter_exact_match: bool,
    /// 缓存对应的 ZSet 排序字段
    zset_sort_by: ZSetSortBy,
    /// 缓存对应的排序方向
    sort_order: SortOrder,
    /// 列表筛选结果
    list: Vec<(usize, String)>,
    /// 集合筛选结果
    set: Vec<(usize, String)>,
    /// ZSet 筛选与排序结果
    zset: Vec<(usize, ZSetMember)>,
    /// Hash 筛选结果
    hash: Vec<(usize, HashField)>,
    /// ZSet 分数范围（用于可视化柱状图），随筛选结果一起缓存
    zset_score_range: (f64, f64),
}

/// 计算 ZSet 分数范围，用于可视化柱状图
fn zset_score_range(members: &[(usize, ZSetMember)]) -> (f64, f64) {
    if members.is_empty() {
        return (0.0, 1.0);
    }
    let min = members
        .iter()
        .map(|(_, member)| member.score)
        .fold(f64::INFINITY, f64::min);
    let max = members
        .iter()
        .map(|(_, member)| member.score)
        .fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < f64::EPSILON {
        (min - 1.0, max + 1.0)
    } else {
        (min, max)
    }
}

/// 生成筛选用的查询串。
///
/// 精确匹配保留原文，模糊匹配统一转小写，与筛选框的既有语义一致。
fn filter_query(filter_text: &str, filter_exact_match: bool) -> String {
    if filter_exact_match {
        filter_text.to_string()
    } else {
        filter_text.to_lowercase()
    }
}

/// 应用筛选到字符串列表
fn filter_strings(
    items: &[String],
    filter_text: &str,
    filter_exact_match: bool,
) -> Vec<(usize, String)> {
    if filter_text.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx, item.clone()))
            .collect();
    }
    let query = filter_query(filter_text, filter_exact_match);
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if filter_exact_match {
                item.contains(&query)
            } else {
                item.to_lowercase().contains(&query)
            }
        })
        .map(|(idx, item)| (idx, item.clone()))
        .collect()
}

/// 应用筛选到 Hash 字段
fn filter_hash_fields(
    items: &[HashField],
    filter_text: &str,
    filter_exact_match: bool,
) -> Vec<(usize, HashField)> {
    if filter_text.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx, item.clone()))
            .collect();
    }
    let query = filter_query(filter_text, filter_exact_match);
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let search_text = format!("{} {}", item.field, item.value);
            if filter_exact_match {
                search_text.contains(&query)
            } else {
                search_text.to_lowercase().contains(&query)
            }
        })
        .map(|(idx, item)| (idx, item.clone()))
        .collect()
}

/// 应用筛选到 ZSet 成员
fn filter_zset_members(
    items: &[ZSetMember],
    filter_text: &str,
    filter_exact_match: bool,
    zset_sort_by: ZSetSortBy,
    sort_order: SortOrder,
) -> Vec<(usize, ZSetMember)> {
    let mut filtered: Vec<(usize, ZSetMember)> = if filter_text.is_empty() {
        items
            .iter()
            .enumerate()
            .map(|(idx, member)| (idx, member.clone()))
            .collect()
    } else {
        let query = filter_query(filter_text, filter_exact_match);
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if filter_exact_match {
                    item.member.contains(&query)
                } else {
                    item.member.to_lowercase().contains(&query)
                }
            })
            .map(|(idx, member)| (idx, member.clone()))
            .collect()
    };

    // 应用排序
    match zset_sort_by {
        ZSetSortBy::Score => {
            filtered.sort_by(|a, b| {
                if sort_order == SortOrder::Asc {
                    a.1.score
                        .partial_cmp(&b.1.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    b.1.score
                        .partial_cmp(&a.1.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }
        ZSetSortBy::Member => {
            filtered.sort_by(|a, b| {
                if sort_order == SortOrder::Asc {
                    a.1.member.cmp(&b.1.member)
                } else {
                    b.1.member.cmp(&a.1.member)
                }
            });
        }
    }
    filtered
}

/// 键值视图
pub struct KeyValueView {
    /// 当前连接 ID
    connection_id: Option<String>,
    /// 当前数据库索引
    db_index: u8,
    /// 当前键名
    current_key: Option<String>,
    /// 键信息
    key_info: Option<KeyInfo>,
    /// 键值内容
    value_content: Option<KeyValueContent>,
    /// 键值内容版本号，内容变化时递增（用于失效筛选结果缓存）
    value_content_version: u64,
    /// 筛选/排序结果缓存
    filtered_content: FilteredContent,
    /// 加载状态
    load_state: LoadState,
    /// 焦点句柄
    focus_handle: FocusHandle,
    /// 是否修改过
    is_dirty: bool,
    /// 当前查看格式
    view_format: ViewFormat,
    /// 查看格式选择器状态
    format_select: Entity<SelectState<Vec<ViewFormat>>>,
    /// String 值编辑器状态
    string_editor: Entity<InputState>,
    /// 待设置的编辑器值（异步加载完成后设置）
    pending_editor_value: Option<String>,

    // === 筛选功能 ===
    /// 筛选输入框状态
    filter_input: Entity<InputState>,
    /// 当前筛选文本
    filter_text: String,
    /// 是否全文匹配
    filter_exact_match: bool,

    // === 排序功能 ===
    /// 排序方向
    sort_order: SortOrder,
    /// ZSet 排序字段
    zset_sort_by: ZSetSortBy,

    // === List 插入位置 ===
    list_insert_position: ListInsertPosition,
    /// 是否允许关闭标签页
    closeable: bool,
}

impl KeyValueView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_closeable(false, window, cx)
    }

    pub fn new_with_closeable(
        closeable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let format_select = cx.new(|cx| {
            SelectState::new(
                ViewFormat::all(),
                Some(IndexPath::default().row(0)),
                window,
                cx,
            )
        });

        let string_editor = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .code_editor(Language::from_str("text"))
                .line_number(true)
                .searchable(true)
                .soft_wrap(true)
                .placeholder(t!("KeyValueView.select_key_placeholder").to_string())
        });

        let filter_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("KeyValueView.filter_placeholder").to_string())
        });

        cx.subscribe_in(&format_select, window, Self::on_format_changed)
            .detach();

        cx.subscribe_in(&string_editor, window, Self::on_editor_changed)
            .detach();

        cx.subscribe_in(&filter_input, window, Self::on_filter_changed)
            .detach();

        Self {
            connection_id: None,
            db_index: 0,
            current_key: None,
            key_info: None,
            value_content: None,
            value_content_version: 0,
            filtered_content: FilteredContent::default(),
            load_state: LoadState::Empty,
            focus_handle: cx.focus_handle(),
            is_dirty: false,
            view_format: ViewFormat::Raw,
            format_select,
            string_editor,
            pending_editor_value: None,
            filter_input,
            filter_text: String::new(),
            filter_exact_match: false,
            sort_order: SortOrder::Asc,
            zset_sort_by: ZSetSortBy::Score,
            list_insert_position: ListInsertPosition::Tail,
            closeable,
        }
    }

    /// 格式选择器变化处理
    fn on_format_changed(
        &mut self,
        _select: &Entity<SelectState<Vec<ViewFormat>>>,
        event: &SelectEvent<Vec<ViewFormat>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let SelectEvent::Confirm(Some(format)) = event {
            self.view_format = *format.value();
            self.update_editor_content(window, cx);
            self.update_editor_highlighter(cx);
            cx.notify();
        }
    }

    /// 编辑器内容变化处理
    fn on_editor_changed(
        &mut self,
        _editor: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Change = event {
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// 筛选输入变化处理
    fn on_filter_changed(
        &mut self,
        _input: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Change = event {
            self.filter_text = self.filter_input.read(cx).text().to_string();
            cx.notify();
        }
    }

    /// 刷新筛选/排序结果缓存。
    ///
    /// 只有内容版本、筛选条件或排序条件变化时才重新筛选，
    /// 否则直接复用上一次的结果，避免每帧遍历并克隆全部成员。
    fn refresh_filtered_content(&mut self) {
        // 先做命中检查（不复制字符串），避免每帧产生一次无谓分配
        if self.filtered_content.content_version == self.value_content_version
            && self.filtered_content.filter_text == self.filter_text
            && self.filtered_content.filter_exact_match == self.filter_exact_match
            && self.filtered_content.zset_sort_by == self.zset_sort_by
            && self.filtered_content.sort_order == self.sort_order
        {
            return;
        }

        let filter_text = self.filter_text.clone();
        let filter_exact_match = self.filter_exact_match;
        let zset_sort_by = self.zset_sort_by;
        let sort_order = self.sort_order;
        let content_version = self.value_content_version;

        // 拆分借用：内容只读，缓存可写
        let Self {
            value_content,
            filtered_content,
            ..
        } = self;
        filtered_content.content_version = content_version;
        filtered_content.filter_text = filter_text.clone();
        filtered_content.filter_exact_match = filter_exact_match;
        filtered_content.zset_sort_by = zset_sort_by;
        filtered_content.sort_order = sort_order;
        // 只保留当前类型的筛选结果，避免切换键后旧类型数据滞留
        filtered_content.list.clear();
        filtered_content.set.clear();
        filtered_content.zset.clear();
        filtered_content.hash.clear();

        match value_content {
            Some(KeyValueContent::List(items)) => {
                filtered_content.list = filter_strings(items, &filter_text, filter_exact_match);
            }
            Some(KeyValueContent::Set(items)) => {
                filtered_content.set = filter_strings(items, &filter_text, filter_exact_match);
            }
            Some(KeyValueContent::ZSet(items)) => {
                filtered_content.zset = filter_zset_members(
                    items,
                    &filter_text,
                    filter_exact_match,
                    zset_sort_by,
                    sort_order,
                );
                let range = zset_score_range(&filtered_content.zset);
                filtered_content.zset_score_range = range;
            }
            Some(KeyValueContent::Hash(items)) => {
                filtered_content.hash = filter_hash_fields(items, &filter_text, filter_exact_match);
            }
            _ => {}
        }
    }

    /// 切换排序方向
    fn toggle_sort_order(&mut self, cx: &mut Context<Self>) {
        self.sort_order = match self.sort_order {
            SortOrder::Asc => SortOrder::Desc,
            SortOrder::Desc => SortOrder::Asc,
        };
        cx.notify();
    }

    /// 切换 ZSet 排序字段
    fn toggle_zset_sort_by(&mut self, cx: &mut Context<Self>) {
        self.zset_sort_by = match self.zset_sort_by {
            ZSetSortBy::Score => ZSetSortBy::Member,
            ZSetSortBy::Member => ZSetSortBy::Score,
        };
        cx.notify();
    }

    fn large_text_editor_value(
        editor: &Entity<LargeTextEditor>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        match editor.read(cx).get_writeback_text(cx) {
            Ok(value) => Some(value),
            Err(err) => {
                window.push_notification(err.to_string(), cx);
                None
            }
        }
    }

    fn show_large_text_preview_dialog(
        &self,
        title: String,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = create_large_text_editor_with_content(Some(value), window, cx);

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title(SharedString::from(title.clone()))
                .w(px(900.))
                .h(px(680.))
                .child(v_flex().size_full().child(editor.clone()))
                .close_button(true)
                .overlay(false)
                .content_center()
        });
    }

    /// 更新编辑器内容（根据格式转换）
    fn update_editor_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(KeyValueContent::String(ref value)) = self.value_content else {
            return;
        };

        let formatted = self.format_value(value);

        self.string_editor.update(cx, |state, cx| {
            state.set_value(formatted, window, cx);
        });
    }

    /// 更新编辑器高亮语言
    fn update_editor_highlighter(&mut self, cx: &mut Context<Self>) {
        let language = match self.view_format {
            ViewFormat::Json => Language::Json,
            _ => Language::from_str("text"),
        };

        self.string_editor.update(cx, |state, cx| {
            state.set_highlighter(language, cx);
        });
    }

    /// 格式化值
    fn format_value(&self, value: &str) -> String {
        match self.view_format {
            ViewFormat::Raw => value.to_string(),
            ViewFormat::Json => match serde_json::from_str::<serde_json::Value>(value) {
                Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| value.to_string()),
                Err(_) => value.to_string(),
            },
            ViewFormat::Hex => value
                .bytes()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" "),
            ViewFormat::Binary => value
                .bytes()
                .map(|b| format!("{:08b}", b))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// 加载键
    pub fn load_key(
        &mut self,
        connection_id: String,
        db_index: u8,
        key: String,
        cx: &mut Context<Self>,
    ) {
        self.connection_id = Some(connection_id.clone());
        self.db_index = db_index;
        self.current_key = Some(key.clone());
        self.load_state = LoadState::Loading;
        self.is_dirty = false;
        self.pending_editor_value = None;
        cx.notify();

        let global_state = cx.global::<GlobalRedisState>().clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result =
                Tokio::spawn_result(cx, {
                    let connection_id = connection_id.clone();
                    let key = key.clone();
                    async move {
                        Self::fetch_key_value(&global_state, &connection_id, db_index, &key).await
                    }
                })
                .await;

            _ = this.update(cx, |view, cx| {
                match result {
                    Ok(detail) => {
                        if let KeyValueContent::String(ref value) = detail.value {
                            view.pending_editor_value = Some(value.clone());
                        }
                        view.key_info = Some(detail.key_info);
                        view.value_content = Some(detail.value);
                        // 内容变化后筛选结果缓存失效
                        view.value_content_version = view.value_content_version.wrapping_add(1);
                        view.load_state = LoadState::Loaded;
                    }
                    Err(e) => {
                        view.load_state = LoadState::Error(format!("{e:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 在 render 中应用待设置的编辑器值
    fn apply_pending_editor_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(value) = self.pending_editor_value.take() {
            let formatted = self.format_value(&value);
            self.string_editor.update(cx, |state, cx| {
                state.set_value(formatted, window, cx);
            });
        }
    }

    /// 获取键值
    async fn fetch_key_value(
        global_state: &GlobalRedisState,
        connection_id: &str,
        db_index: u8,
        key: &str,
    ) -> anyhow::Result<KeyValueDetail> {
        let conn = global_state
            .get_connection(connection_id)
            .ok_or_else(|| anyhow::anyhow!("{}", t!("KeyValueView.connection_missing")))?;

        let guard = conn.read().await;
        guard
            .get_key_value_detail_in_db(db_index, key)
            .await
            .map_err(anyhow::Error::new)
    }

    /// 获取编辑器内容
    fn get_editor_content(&self, cx: &App) -> String {
        self.string_editor.read(cx).text().to_string()
    }

    /// 渲染键信息面板（工具栏）
    fn render_key_info(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(info) = &self.key_info else {
            return div().into_any_element();
        };
        let section_bg = content_section_bg(cx);

        let key_name = info.name.clone();
        let key_type = info.key_type;
        let key_type_display = info.key_type.display_name();
        let ttl_display = info.ttl_display();
        let view = cx.entity().clone();
        let key_for_copy = key_name.clone();
        let editor_content = self.get_editor_content(cx);
        let can_add_element = matches!(
            key_type,
            RedisKeyType::List | RedisKeyType::Set | RedisKeyType::ZSet | RedisKeyType::Hash
        );
        let is_string = matches!(key_type, RedisKeyType::String);
        let is_zset = matches!(key_type, RedisKeyType::ZSet);
        let zset_sort_label = match self.zset_sort_by {
            ZSetSortBy::Score => t!("KeyValueView.sort_by_score"),
            ZSetSortBy::Member => t!("KeyValueView.sort_by_member"),
        };
        let zset_sort_text = t!("KeyValueView.sort_by", by = zset_sort_label);

        v_flex()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(section_bg)
            // 第一行：类型 + 键名 + 操作按钮
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .items_center()
                    // 类型徽章
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded(px(4.0))
                            .bg(cx.theme().primary)
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(cx.theme().primary_foreground)
                            .child(key_type_display),
                    )
                    // 键名
                    .child(div().flex_1().text_sm().truncate().child(key_name.clone()))
                    // 刷新按钮
                    .child(
                        Button::new("refresh-key")
                            .icon(IconName::Refresh)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |view, cx| {
                                        if let (Some(conn_id), Some(key)) =
                                            (view.connection_id.clone(), view.current_key.clone())
                                        {
                                            view.load_key(conn_id, view.db_index, key, cx);
                                        }
                                    });
                                }
                            }),
                    )
                    // 复制键名按钮
                    .child(
                        Button::new("copy-key")
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    key_for_copy.clone(),
                                ));
                            }),
                    )
                    // TTL 显示（可点击编辑）
                    .child(
                        Button::new("ttl-display")
                            .ghost()
                            .with_size(Size::Medium)
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .px_2()
                                    .py_0p5()
                                    .rounded(px(4.0))
                                    .bg(cx.theme().secondary)
                                    .child(
                                        Icon::new(IconName::Calendar)
                                            .with_size(Size::Small)
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().secondary_foreground)
                                            .child(ttl_display),
                                    ),
                            )
                            .on_click({
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.show_ttl_dialog(window, cx);
                                    });
                                }
                            }),
                    )
                    // 重命名按钮
                    .child(
                        Button::new("rename-key")
                            .icon(IconName::Edit)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.show_rename_dialog(window, cx);
                                    });
                                }
                            }),
                    )
                    // 删除按钮
                    .child(
                        Button::new("delete-key")
                            .icon(IconName::Remove)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |view, cx| {
                                        cx.emit(KeyValueViewEvent::ValueDeleted {
                                            key: view.current_key.clone().unwrap_or_default(),
                                        });
                                    });
                                }
                            }),
                    ),
            )
            // 第二行：筛选 + 操作按钮
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .pb_2()
                    .gap_2()
                    .items_center()
                    // 左侧：筛选输入框（集合类型）或格式选择器（String 类型）
                    .child(
                        h_flex()
                            .flex_1()
                            .gap_2()
                            .items_center()
                            .when(is_string, |this| {
                                this.child(
                                    div()
                                        .text_base()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t!("KeyValueView.view_mode").to_string()),
                                )
                                .child(
                                    Select::new(&self.format_select)
                                        .with_size(Size::Medium)
                                        .w(px(100.)),
                                )
                            })
                            .when(can_add_element, |this| {
                                this.child(
                                    Input::new(&self.filter_input)
                                        .with_size(Size::Medium)
                                        .w(px(200.)),
                                )
                                .child(
                                    Checkbox::new("exact-match")
                                        .label(t!("KeyValueView.exact_match").to_string())
                                        .with_size(Size::Medium)
                                        .checked(self.filter_exact_match)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.filter_exact_match =
                                                        !view.filter_exact_match;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                            }),
                    )
                    // 右侧按钮组
                    .child(
                        h_flex()
                            .gap_2()
                            // 排序按钮（集合类型）
                            .when(can_add_element, |this| {
                                this.child(
                                    Button::new("sort-order")
                                        .icon(if self.sort_order == SortOrder::Asc {
                                            IconName::SortAscending
                                        } else {
                                            IconName::SortDescending
                                        })
                                        .ghost()
                                        .with_size(Size::Medium)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.toggle_sort_order(cx);
                                                });
                                            }
                                        }),
                                )
                            })
                            .when(is_zset, |this| {
                                this.child(
                                    Button::new("zset-sort-by")
                                        .icon(IconName::ChevronsUpDown)
                                        .label(zset_sort_text.to_string())
                                        .ghost()
                                        .with_size(Size::Medium)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.toggle_zset_sort_by(cx);
                                                });
                                            }
                                        }),
                                )
                            })
                            // 插入行按钮（仅对集合类型显示）
                            .when(can_add_element, |this| {
                                this.child(
                                    Button::new("add-element")
                                        .icon(IconName::Plus)
                                        .label(t!("KeyValueView.insert_row").to_string())
                                        .primary()
                                        .with_size(Size::Medium)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, window, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.show_add_dialog(window, cx);
                                                });
                                            }
                                        }),
                                )
                            })
                            // 复制值按钮
                            .child(
                                Button::new("copy-value")
                                    .icon(IconName::Copy)
                                    .label(t!("KeyValueView.copy_value").to_string())
                                    .ghost()
                                    .with_size(Size::Medium)
                                    .on_click({
                                        let content = editor_content.clone();
                                        move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                content.clone(),
                                            ));
                                        }
                                    }),
                            )
                            // 保存按钮（仅 String 类型显示）
                            .when(is_string && self.is_dirty, |this| {
                                this.child(
                                    Button::new("save-value")
                                        .icon(IconName::Check)
                                        .label(t!("KeyValueView.save").to_string())
                                        .success()
                                        .with_size(Size::Medium)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.save_string_value(cx);
                                                });
                                            }
                                        }),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 显示新增对话框（根据当前键类型分发）
    fn show_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = &self.key_info else { return };
        match info.key_type {
            RedisKeyType::List => self.show_list_add_dialog(window, cx),
            RedisKeyType::Set => self.show_set_add_dialog(window, cx),
            RedisKeyType::ZSet => self.show_zset_add_dialog(window, cx),
            RedisKeyType::Hash => self.show_hash_add_dialog(window, cx),
            _ => {}
        }
    }

    // === List 对话框 ===

    /// 显示 List 添加对话框
    fn show_list_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value_editor = create_large_text_editor_with_content(None, window, cx);
        let view = cx.entity().downgrade();
        let position = self.list_insert_position;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor_for_ok = value_editor.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.list_insert_title").to_string())
                .w(px(800.))
                .h(px(620.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_3()
                        .child(
                            h_flex()
                                .gap_4()
                                .child(
                                    Radio::new("insert-head")
                                        .label(t!("KeyValueView.list_insert_head").to_string())
                                        .checked(position == ListInsertPosition::Head),
                                )
                                .child(
                                    Radio::new("insert-tail")
                                        .label(t!("KeyValueView.list_insert_tail").to_string())
                                        .checked(position == ListInsertPosition::Tail),
                                ),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.value_label").to_string()),
                                )
                                .child(div().h(px(460.)).w_full().child(value_editor.clone())),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.add").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(value) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if value.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.add_list_element(value, position, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 显示 List 编辑对话框
    fn show_list_edit_dialog(
        &mut self,
        index: usize,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value_editor = create_large_text_editor_with_content(Some(current_value), window, cx);
        let view = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor_for_ok = value_editor.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.edit_list_item", index = index + 1).to_string())
                .w(px(800.))
                .h(px(600.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("KeyValueView.value_label").to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .child(value_editor.clone()),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.save").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(value) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if value.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.edit_list_element(index, value, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 添加 List 元素
    fn add_list_element(
        &mut self,
        value: String,
        position: ListInsertPosition,
        cx: &mut Context<Self>,
    ) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let value = value.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    match position {
                        ListInsertPosition::Head => {
                            guard
                                .lpush_in_db(db_index, &key, &[value.as_str()])
                                .await
                                .map_err(anyhow::Error::new)?;
                        }
                        ListInsertPosition::Tail => {
                            guard
                                .rpush_in_db(db_index, &key, &[value.as_str()])
                                .await
                                .map_err(anyhow::Error::new)?;
                        }
                    }
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 编辑 List 元素（通过 LSET）
    fn edit_list_element(&mut self, index: usize, new_value: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let new_value = new_value.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .lset_in_db(db_index, &key, index as i64, &new_value)
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // === Set 对话框 ===

    /// 显示 Set 添加对话框
    fn show_set_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let member_editor = create_large_text_editor_with_content(None, window, cx);
        let view = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor_for_ok = member_editor.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.add_set_member").to_string())
                .w(px(800.))
                .h(px(600.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("KeyValueView.member_label").to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .child(member_editor.clone()),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.add").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(member) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if member.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.add_set_member(member, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 显示 Set 编辑对话框
    fn show_set_edit_dialog(
        &mut self,
        current_member: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let member_editor =
            create_large_text_editor_with_content(Some(current_member.clone()), window, cx);
        let view = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor_for_ok = member_editor.clone();
            let view_for_ok = view.clone();
            let old_member = current_member.clone();

            dialog
                .title(t!("KeyValueView.edit_set_member").to_string())
                .w(px(800.))
                .h(px(600.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("KeyValueView.member_label").to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .child(member_editor.clone()),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.save").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(new_member) =
                        Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if new_member.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.update_set_member(old_member.clone(), new_member, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 添加 Set 成员
    fn add_set_member(&mut self, member: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let member = member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .sadd_in_db(db_index, &key, &[member.as_str()])
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // === ZSet 对话框 ===

    /// 显示 ZSet 添加对话框
    fn show_zset_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let score_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("KeyValueView.score_placeholder").to_string());
            state.set_value("0", window, cx);
            state
        });
        let member_editor = create_large_text_editor_with_content(None, window, cx);
        let view = cx.entity().downgrade();

        // 在打开对话框前设置焦点，避免闪烁
        score_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let score_for_ok = score_input.clone();
            let editor_for_ok = member_editor.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.add_zset_member").to_string())
                .w(px(800.))
                .h(px(660.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.score_label").to_string()),
                                )
                                .child(Input::new(&score_input).w_full()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.member_label").to_string()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .w_full()
                                        .child(member_editor.clone()),
                                ),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.add").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(member) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    let score_str = score_for_ok.read(cx).text().to_string();
                    let score: f64 = score_str.parse().unwrap_or(0.0);
                    if member.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.add_zset_member(member, score, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 显示 ZSet 编辑对话框
    fn show_zset_edit_dialog(
        &mut self,
        member: String,
        current_score: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let score_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("KeyValueView.score_placeholder").to_string());
            state.set_value(format!("{}", current_score), window, cx);
            state
        });
        let member_editor = create_large_text_editor_with_content(Some(member.clone()), window, cx);
        let view = cx.entity().downgrade();
        let old_member = member;

        // 在打开对话框前设置焦点，避免闪烁
        score_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let score_for_ok = score_input.clone();
            let editor_for_ok = member_editor.clone();
            let view_for_ok = view.clone();
            let old_member_for_ok = old_member.clone();

            dialog
                .title(t!("KeyValueView.edit_zset_member").to_string())
                .w(px(800.))
                .h(px(660.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.score_label").to_string()),
                                )
                                .child(Input::new(&score_input).w_full()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.member_label").to_string()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .w_full()
                                        .child(member_editor.clone()),
                                ),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.save").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let Some(new_member) =
                        Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    let score_str = score_for_ok.read(cx).text().to_string();
                    let score: f64 = score_str.parse().unwrap_or(0.0);
                    if new_member.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.update_zset_member(old_member_for_ok.clone(), new_member, score, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 添加 ZSet 成员
    fn add_zset_member(&mut self, member: String, score: f64, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let member = member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .zadd_in_db(db_index, &key, &[(score, member.as_str())])
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 更新 ZSet 成员（删除旧成员后添加新成员）
    fn update_zset_member(
        &mut self,
        old_member: String,
        new_member: String,
        score: f64,
        cx: &mut Context<Self>,
    ) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let old_member = old_member.clone();
                let new_member = new_member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    // 如果成员名变了，先删除旧的
                    if old_member != new_member {
                        guard
                            .zrem_in_db(db_index, &key, &[old_member.as_str()])
                            .await
                            .map_err(anyhow::Error::new)?;
                    }
                    guard
                        .zadd_in_db(db_index, &key, &[(score, new_member.as_str())])
                        .await
                        .map_err(anyhow::Error::new)?;
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // === Hash 对话框 ===

    /// 显示 Hash 添加对话框
    fn show_hash_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("KeyValueView.hash_field_placeholder").to_string())
        });
        let value_editor = create_large_text_editor_with_content(None, window, cx);
        let view = cx.entity().downgrade();

        // 在打开对话框前设置焦点，避免闪烁
        field_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let field_for_ok = field_input.clone();
            let editor_for_ok = value_editor.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.add_hash_field").to_string())
                .w(px(800.))
                .h(px(660.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.field_label").to_string()),
                                )
                                .child(Input::new(&field_input).w_full()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.value_label").to_string()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .w_full()
                                        .child(value_editor.clone()),
                                ),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.add").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let field = field_for_ok.read(cx).text().to_string();
                    let Some(value) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if field.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.set_hash_field(field, value, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 显示 Hash 编辑对话框
    fn show_hash_edit_dialog(
        &mut self,
        field: String,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("KeyValueView.hash_field_placeholder_edit").to_string());
            state.set_value(field.clone(), window, cx);
            state
        });
        let value_editor = create_large_text_editor_with_content(Some(current_value), window, cx);
        let view = cx.entity().downgrade();
        let old_field = field;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let field_for_ok = field_input.clone();
            let editor_for_ok = value_editor.clone();
            let view_for_ok = view.clone();
            let old_field_for_ok = old_field.clone();

            dialog
                .title(t!("KeyValueView.edit_hash_field").to_string())
                .w(px(800.))
                .h(px(660.))
                .child(
                    v_flex()
                        .size_full()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.field_label").to_string()),
                                )
                                .child(Input::new(&field_input).w_full()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.value_label").to_string()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .w_full()
                                        .child(value_editor.clone()),
                                ),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("KeyValueView.save").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let new_field = field_for_ok.read(cx).text().to_string();
                    let Some(value) = Self::large_text_editor_value(&editor_for_ok, window, cx)
                    else {
                        return false;
                    };
                    if new_field.is_empty() {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        // 如果字段名变了，先删除旧字段
                        if old_field_for_ok != new_field {
                            v.delete_hash_field_then_set(
                                old_field_for_ok.clone(),
                                new_field.clone(),
                                value.clone(),
                                cx,
                            );
                        } else {
                            v.set_hash_field(new_field, value, cx);
                        }
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 设置 Hash 字段
    fn set_hash_field(&mut self, field: String, value: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let field = field.clone();
                let value = value.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .hset_in_db(db_index, &key, &field, &value)
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 删除旧 Hash 字段并设置新字段
    fn delete_hash_field_then_set(
        &mut self,
        old_field: String,
        new_field: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .hdel_in_db(db_index, &key, &[old_field.as_str()])
                        .await
                        .map_err(anyhow::Error::new)?;
                    guard
                        .hset_in_db(db_index, &key, &new_field, &value)
                        .await
                        .map_err(anyhow::Error::new)?;
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // === TTL 对话框 ===

    /// 显示 TTL 设置对话框
    fn show_ttl_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current_ttl = self.key_info.as_ref().map(|i| i.ttl).unwrap_or(-1);
        let is_permanent = current_ttl == -1;

        let ttl_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("KeyValueView.ttl_placeholder").to_string());
            if current_ttl > 0 {
                state.set_value(current_ttl.to_string(), window, cx);
            }
            state
        });
        let view = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let ttl_for_ok = ttl_input.clone();
            let view_for_ok = view.clone();

            dialog
                .title(t!("KeyValueView.ttl_title").to_string())
                .w(px(400.))
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(if is_permanent {
                                    t!("KeyValueView.ttl_current_permanent").to_string()
                                } else {
                                    t!("KeyValueView.ttl_current", ttl = current_ttl).to_string()
                                }),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .child(t!("KeyValueView.ttl_label").to_string()),
                                )
                                .child(Input::new(&ttl_input).w_full()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("KeyValueView.ttl_hint").to_string()),
                        ),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("Common.ok").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let ttl_str = ttl_for_ok.read(cx).text().to_string();
                    let ttl: Option<i64> = if ttl_str.is_empty() || ttl_str == "-1" {
                        None // 永久
                    } else {
                        ttl_str.parse().ok()
                    };
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.set_key_ttl(ttl, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 设置键的 TTL
    fn set_key_ttl(&mut self, ttl: Option<i64>, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    match ttl {
                        Some(seconds) if seconds > 0 => {
                            guard
                                .expire_in_db(db_index, &key, seconds)
                                .await
                                .map_err(anyhow::Error::new)?;
                        }
                        _ => {
                            guard
                                .persist_in_db(db_index, &key)
                                .await
                                .map_err(anyhow::Error::new)?;
                        }
                    }
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // === 重命名键 ===

    /// 显示重命名对话框
    fn show_rename_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current_name) = self.current_key.clone() else {
            return;
        };
        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("KeyValueView.new_key_name_placeholder").to_string());
            state.set_value(current_name.clone(), window, cx);
            state
        });
        let view = cx.entity().downgrade();

        // 在打开对话框前设置焦点，避免闪烁
        name_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let input_for_ok = name_input.clone();
            let view_for_ok = view.clone();
            let old_name = current_name.clone();

            dialog
                .title(t!("KeyValueView.rename_key_title").to_string())
                .w(px(400.))
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("KeyValueView.new_key_name_label").to_string()),
                        )
                        .child(Input::new(&name_input).w_full()),
                )
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("Common.ok").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, window, cx| {
                    let new_name = input_for_ok.read(cx).text().to_string();
                    if new_name.is_empty() || new_name == old_name {
                        return false;
                    }
                    let _ = view_for_ok.update(cx, |v, cx| {
                        v.rename_key(new_name, cx);
                        window.close_dialog(cx);
                    });
                    false
                })
        });
    }

    /// 重命名键
    fn rename_key(&mut self, new_name: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(old_name) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let old_name = old_name.clone();
                let new_name = new_name.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .rename_in_db(db_index, &old_name, &new_name)
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.current_key = Some(new_name.clone());
                    view.load_key(connection_id, db_index, new_name, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 保存 String 值
    fn save_string_value(&mut self, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let value = self.get_editor_content(cx);
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let value = value.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .set_in_db(db_index, &key, &value, None)
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.is_dirty = false;
                    cx.emit(KeyValueViewEvent::ValueUpdated { key });
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 通用删除操作的异步模板
    fn reload_after_operation(&mut self, cx: &mut Context<Self>) {
        if let (Some(conn_id), Some(key)) = (self.connection_id.clone(), self.current_key.clone()) {
            self.load_key(conn_id, self.db_index, key, cx);
        }
    }

    /// 删除 List 元素
    fn delete_list_element(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let delete_marker = "__DELETED_ELEMENT_MARKER__";
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .lset_in_db(db_index, &key, index as i64, delete_marker)
                        .await
                        .map_err(anyhow::Error::new)?;
                    guard
                        .execute_command_in_db(
                            db_index,
                            &format!("LREM {} 1 {}", key, delete_marker),
                        )
                        .await
                        .map_err(anyhow::Error::new)?;
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 删除 Set 元素
    fn delete_set_element(&mut self, member: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let member = member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .srem_in_db(db_index, &key, &[member.as_str()])
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 更新 Set 成员
    fn update_set_member(
        &mut self,
        old_member: String,
        new_member: String,
        cx: &mut Context<Self>,
    ) {
        if !should_replace_set_member(&old_member, &new_member) {
            return;
        }
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let old_member = old_member.clone();
                let new_member = new_member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .srem_in_db(db_index, &key, &[old_member.as_str()])
                        .await
                        .map_err(anyhow::Error::new)?;
                    guard
                        .sadd_in_db(db_index, &key, &[new_member.as_str()])
                        .await
                        .map_err(anyhow::Error::new)?;
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 删除 ZSet 元素
    fn delete_zset_element(&mut self, member: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let member = member.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .zrem_in_db(db_index, &key, &[member.as_str()])
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 删除 Hash 字段
    fn delete_hash_field(&mut self, field: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id.clone() else {
            return;
        };
        let Some(key) = self.current_key.clone() else {
            return;
        };
        let global_state = cx.global::<GlobalRedisState>().clone();
        let db_index = self.db_index;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = Tokio::spawn_result(cx, {
                let connection_id = connection_id.clone();
                let key = key.clone();
                let field = field.clone();
                async move {
                    let conn = global_state.get_connection(&connection_id).ok_or_else(|| {
                        anyhow::anyhow!("{}", t!("KeyValueView.connection_missing"))
                    })?;
                    let guard = conn.read().await;
                    guard
                        .hdel_in_db(db_index, &key, &[field.as_str()])
                        .await
                        .map_err(anyhow::Error::new)
                }
            })
            .await;

            _ = this.update(cx, |view, cx| {
                if result.is_ok() {
                    view.reload_after_operation(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 渲染值编辑器
    fn render_value_editor(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(content) = &self.value_content else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(t!("KeyValueView.select_key_placeholder").to_string())
                .into_any_element();
        };

        match content {
            KeyValueContent::String(_) => self.render_string_editor(cx).into_any_element(),
            KeyValueContent::List(_) => self.render_list_view(cx).into_any_element(),
            KeyValueContent::Set(_) => self.render_set_view(cx).into_any_element(),
            KeyValueContent::ZSet(_) => self.render_zset_view(cx).into_any_element(),
            KeyValueContent::Hash(_) => self.render_hash_view(cx).into_any_element(),
            KeyValueContent::Stream(entries) => {
                self.render_stream_view(entries, cx).into_any_element()
            }
            KeyValueContent::None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(t!("KeyValueView.empty_value").to_string())
                .into_any_element(),
        }
    }

    /// 渲染 String 编辑器（使用 Input 组件）
    fn render_string_editor(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        Input::new(&self.string_editor)
            .size_full()
            .cleanable(false)
            .rounded(px(0.0))
    }

    /// 渲染底部状态栏
    fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let Some(info) = &self.key_info else {
            return div().into_any_element();
        };
        let section_bg = content_section_bg(cx);

        let size = info.size.unwrap_or(0);
        let content_len = match &self.value_content {
            Some(KeyValueContent::String(s)) => s.len(),
            Some(KeyValueContent::List(v)) => v.len(),
            Some(KeyValueContent::Set(v)) => v.len(),
            Some(KeyValueContent::ZSet(v)) => v.len(),
            Some(KeyValueContent::Hash(v)) => v.len(),
            Some(KeyValueContent::Stream(v)) => v.len(),
            _ => 0,
        };

        let memory_display = info
            .memory_usage
            .map(|m| t!("KeyValueView.status_memory", memory = m).to_string())
            .unwrap_or_default();

        h_flex()
            .w_full()
            .h(px(24.0))
            .px_3()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(section_bg)
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        div()
                            .text_base()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                t!("KeyValueView.status_length", count = content_len).to_string(),
                            ),
                    )
                    .when(size > 0, |this| {
                        this.child(
                            div()
                                .text_base()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("KeyValueView.status_size", size = size).to_string()),
                        )
                    })
                    .when(!memory_display.is_empty(), |this| {
                        this.child(
                            div()
                                .text_base()
                                .text_color(cx.theme().muted_foreground)
                                .child(memory_display.clone()),
                        )
                    }),
            )
            .child(
                h_flex().gap_2().child(
                    div()
                        .text_base()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.view_format.display_name()),
                ),
            )
            .into_any_element()
    }

    /// 渲染 List 视图
    fn render_list_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.filtered_content.list.len();

        v_flex()
            .id("list-value-scroll")
            .size_full()
            .child(self.render_table_header(
                vec![
                    (t!("KeyValueView.column_index").to_string(), 60.0),
                    (t!("KeyValueView.column_value").to_string(), 0.0),
                    (t!("KeyValueView.column_action").to_string(), 120.0),
                ],
                cx,
            ))
            .child(
                uniform_list(
                    "list-value-rows",
                    row_count,
                    cx.processor(
                        move |this: &mut Self, visible_range: Range<usize>, _window, cx| {
                            visible_range
                                .map(|ix| this.render_list_row(ix, cx))
                                .collect()
                        },
                    ),
                )
                .flex_1()
                .size_full(),
            )
    }

    /// 渲染 List 单行
    fn render_list_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().clone();
        let Some((idx, item)) = self.filtered_content.list.get(ix) else {
            return div().into_any_element();
        };
        let idx = *idx;
        // SharedString 的克隆只是引用计数，值本身只拷贝一次
        let item = SharedString::from(item.clone());
        let value_for_copy = item.clone();
        let value_for_edit = item.clone();
        let value_for_preview = item.clone();

        h_flex()
            .id(("list-row", idx))
            .group("list-row")
            .w_full()
            .min_h(px(40.0))
            .px_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|this| this.bg(cx.theme().muted))
            .child(
                div()
                    .w(px(60.0))
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{}", idx + 1)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().text_base().truncate().child(item.clone()))
                    .child(
                        Button::new(("preview-list", idx))
                            .icon(IconName::Maximize)
                            .ghost()
                            .with_size(Size::Medium)
                            .tooltip(t!("KeyValueView.view_full_value_tooltip"))
                            .on_click({
                                let view = view.clone();
                                let value = value_for_preview.clone();
                                move |_, window, cx| {
                                    let title = large_text_preview_title(
                                        "List item",
                                        Some(&format!("#{}", idx + 1)),
                                    );
                                    view.update(cx, |v, cx| {
                                        v.show_large_text_preview_dialog(
                                            title,
                                            value.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w(px(120.0))
                    .justify_end()
                    .gap_1()
                    .opacity(0.)
                    .group_hover("list-row", |this| this.opacity(1.))
                    .child(
                        Button::new(("copy-list", idx))
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let value = value_for_copy.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        value.to_string(),
                                    ));
                                }
                            }),
                    )
                    .child(
                        Button::new(("edit-list", idx))
                            .icon(IconName::Edit)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let value = value_for_edit.clone();
                                move |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.show_list_edit_dialog(idx, value.to_string(), window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(("delete-list", idx))
                            .icon(IconName::Remove)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |v, cx| {
                                        v.delete_list_element(idx, cx);
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 渲染 Set 视图
    fn render_set_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.filtered_content.set.len();

        v_flex()
            .id("set-value-scroll")
            .size_full()
            .child(self.render_table_header(
                vec![
                    (t!("KeyValueView.column_member").to_string(), 0.0),
                    (t!("KeyValueView.column_action").to_string(), 120.0),
                ],
                cx,
            ))
            .child(
                uniform_list(
                    "set-value-rows",
                    row_count,
                    cx.processor(
                        move |this: &mut Self, visible_range: Range<usize>, _window, cx| {
                            visible_range
                                .map(|ix| this.render_set_row(ix, cx))
                                .collect()
                        },
                    ),
                )
                .flex_1()
                .size_full(),
            )
    }

    /// 渲染 Set 单行
    fn render_set_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().clone();
        let Some((idx, item)) = self.filtered_content.set.get(ix) else {
            return div().into_any_element();
        };
        let idx = *idx;
        // SharedString 的克隆只是引用计数，值本身只拷贝一次
        let item = SharedString::from(item.clone());
        let value_for_copy = item.clone();
        let value_for_delete = item.clone();
        let value_for_edit = item.clone();
        let value_for_preview = item.clone();

        h_flex()
            .id(("set-row", idx))
            .group("set-row")
            .w_full()
            .min_h(px(40.0))
            .px_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|this| this.bg(cx.theme().muted))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(IconName::Minus)
                            .with_size(Size::Small)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().flex_1().text_base().truncate().child(item.clone()))
                    .child(
                        Button::new(("preview-set", idx))
                            .icon(IconName::Maximize)
                            .ghost()
                            .with_size(Size::Medium)
                            .tooltip(t!("KeyValueView.view_full_value_tooltip"))
                            .on_click({
                                let view = view.clone();
                                let value = value_for_preview.clone();
                                move |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.show_large_text_preview_dialog(
                                            large_text_preview_title("Set member", None),
                                            value.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w(px(120.0))
                    .justify_end()
                    .gap_1()
                    .opacity(0.)
                    .group_hover("set-row", |this| this.opacity(1.))
                    .child(
                        Button::new(("copy-set", idx))
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let value = value_for_copy.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        value.to_string(),
                                    ));
                                }
                            }),
                    )
                    .child(
                        Button::new(("edit-set", idx))
                            .icon(IconName::Edit)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let member = value_for_edit.clone();
                                move |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.show_set_edit_dialog(member.to_string(), window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(("delete-set", idx))
                            .icon(IconName::Remove)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let member = value_for_delete.clone();
                                move |_, _, cx| {
                                    view.update(cx, |v, cx| {
                                        v.delete_set_element(member.to_string(), cx);
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 渲染 ZSet 视图
    fn render_zset_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.filtered_content.zset.len();
        // 分数范围随筛选结果一起缓存，避免每帧全量遍历
        let (min_score, max_score) = self.filtered_content.zset_score_range;

        v_flex()
            .id("zset-value-scroll")
            .size_full()
            .child(self.render_table_header(
                vec![
                    (t!("KeyValueView.column_rank").to_string(), 50.0),
                    (t!("KeyValueView.column_score").to_string(), 140.0),
                    (t!("KeyValueView.column_member").to_string(), 0.0),
                    (t!("KeyValueView.column_action").to_string(), 120.0),
                ],
                cx,
            ))
            .child(
                uniform_list(
                    "zset-value-rows",
                    row_count,
                    cx.processor(
                        move |this: &mut Self, visible_range: Range<usize>, _window, cx| {
                            visible_range
                                .map(|ix| this.render_zset_row(ix, min_score, max_score, cx))
                                .collect()
                        },
                    ),
                )
                .flex_1()
                .size_full(),
            )
    }

    /// 渲染 ZSet 单行
    fn render_zset_row(
        &mut self,
        ix: usize,
        min_score: f64,
        max_score: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().clone();
        let Some((original_idx, item)) = self.filtered_content.zset.get(ix) else {
            return div().into_any_element();
        };
        let original_idx = *original_idx;
        let score = item.score;
        // SharedString 的克隆只是引用计数，值本身只拷贝一次
        let member = SharedString::from(item.member.clone());
        let member_for_edit = member.clone();
        let member_for_delete = member.clone();
        let member_for_preview = member.clone();
        let display_idx = ix;

        // 计算分数百分比用于可视化柱状图 (0.0-1.0)
        let score_ratio = if (max_score - min_score).abs() < f64::EPSILON {
            0.5
        } else {
            ((score - min_score) / (max_score - min_score)).clamp(0.05, 1.0)
        };

        // 排名徽章
        let rank_display = match display_idx {
            0 => "🥇".to_string(),
            1 => "🥈".to_string(),
            2 => "🥉".to_string(),
            n => format!("{}", n + 1),
        };

        h_flex()
            .id(("zset-row", original_idx))
            .group("zset-row")
            .w_full()
            .min_h(px(40.0))
            .px_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|this| this.bg(cx.theme().muted))
            // 排名徽章
            .child(
                div()
                    .w(px(50.0))
                    .text_sm()
                    .font_weight(if display_idx < 3 {
                        gpui::FontWeight::BOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(if display_idx < 3 {
                        cx.theme().primary
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(rank_display),
            )
            // 分数可视化柱状图
            .child(
                h_flex()
                    .w(px(140.0))
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .h(px(16.0))
                            .w(px(60.0))
                            .rounded(px(2.0))
                            .bg(cx.theme().muted)
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(score_ratio as f32))
                                    .rounded(px(2.0))
                                    .bg(cx.theme().primary),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().primary)
                            .child(format!("{:.2}", score)),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().text_base().truncate().child(member.clone()))
                    .child(
                        Button::new(("preview-zset", original_idx))
                            .icon(IconName::Maximize)
                            .ghost()
                            .with_size(Size::Medium)
                            .tooltip(t!("KeyValueView.view_full_value_tooltip"))
                            .on_click({
                                let view = view.clone();
                                let value = member_for_preview.clone();
                                move |_, window, cx| {
                                    let title = large_text_preview_title(
                                        "ZSet member",
                                        Some(&format!("score {:.2}", score)),
                                    );
                                    view.update(cx, |v, cx| {
                                        v.show_large_text_preview_dialog(
                                            title,
                                            value.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w(px(120.0))
                    .justify_end()
                    .gap_1()
                    .opacity(0.)
                    .group_hover("zset-row", |this| this.opacity(1.))
                    .child(
                        Button::new(("copy-zset", original_idx))
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let value = member_for_preview.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                        "{}: {}",
                                        score,
                                        value.as_str()
                                    )));
                                }
                            }),
                    )
                    .child(
                        Button::new(("edit-zset", original_idx))
                            .icon(IconName::Edit)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let member = member_for_edit.clone();
                                move |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.show_zset_edit_dialog(
                                            member.to_string(),
                                            score,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(("delete-zset", original_idx))
                            .icon(IconName::Remove)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let member = member_for_delete.clone();
                                move |_, _, cx| {
                                    view.update(cx, |v, cx| {
                                        v.delete_zset_element(member.to_string(), cx);
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 渲染 Hash 视图
    fn render_hash_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.filtered_content.hash.len();

        v_flex()
            .id("hash-value-scroll")
            .size_full()
            .child(self.render_table_header(
                vec![
                    (t!("KeyValueView.column_field").to_string(), 150.0),
                    (t!("KeyValueView.column_value").to_string(), 0.0),
                    (t!("KeyValueView.column_action").to_string(), 120.0),
                ],
                cx,
            ))
            .child(
                uniform_list(
                    "hash-value-rows",
                    row_count,
                    cx.processor(
                        move |this: &mut Self, visible_range: Range<usize>, _window, cx| {
                            visible_range
                                .map(|ix| this.render_hash_row(ix, cx))
                                .collect()
                        },
                    ),
                )
                .flex_1()
                .size_full(),
            )
    }

    /// 渲染 Hash 单行
    fn render_hash_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().clone();
        let Some((idx, item)) = self.filtered_content.hash.get(ix) else {
            return div().into_any_element();
        };
        let idx = *idx;
        // SharedString 的克隆只是引用计数，值本身只拷贝一次
        let field = SharedString::from(item.field.clone());
        let value = SharedString::from(item.value.clone());
        let field_for_edit = field.clone();
        let value_for_edit = value.clone();
        let field_for_delete = field.clone();
        let value_for_preview = value.clone();

        h_flex()
            .id(("hash-row", idx))
            .group("hash-row")
            .w_full()
            .min_h(px(40.0))
            .px_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|this| this.bg(cx.theme().muted))
            .child(
                div()
                    .w(px(150.0))
                    .text_base()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .truncate()
                    .child(field.clone()),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().text_base().truncate().child(value.clone()))
                    .child(
                        Button::new(("preview-hash", idx))
                            .icon(IconName::Maximize)
                            .ghost()
                            .with_size(Size::Medium)
                            .tooltip(t!("KeyValueView.view_full_value_tooltip"))
                            .on_click({
                                let view = view.clone();
                                let field = field.clone();
                                let value = value_for_preview.clone();
                                move |_, window, cx| {
                                    let title = large_text_preview_title(
                                        "Hash value",
                                        Some(field.as_str()),
                                    );
                                    view.update(cx, |v, cx| {
                                        v.show_large_text_preview_dialog(
                                            title,
                                            value.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w(px(120.0))
                    .justify_end()
                    .gap_1()
                    .opacity(0.)
                    .group_hover("hash-row", |this| this.opacity(1.))
                    .child(
                        Button::new(("copy-hash", idx))
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let field = field.clone();
                                let value = value.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                        "{}: {}",
                                        field.as_str(),
                                        value.as_str()
                                    )));
                                }
                            }),
                    )
                    .child(
                        Button::new(("edit-hash", idx))
                            .icon(IconName::Edit)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let field = field_for_edit.clone();
                                let value = value_for_edit.clone();
                                move |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.show_hash_edit_dialog(
                                            field.to_string(),
                                            value.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(("delete-hash", idx))
                            .icon(IconName::Remove)
                            .ghost()
                            .with_size(Size::Medium)
                            .on_click({
                                let view = view.clone();
                                let field = field_for_delete.clone();
                                move |_, _, cx| {
                                    view.update(cx, |v, cx| {
                                        v.delete_hash_field(field.to_string(), cx);
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 渲染表格头部
    fn render_table_header(
        &self,
        columns: Vec<(String, f32)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let section_bg = content_section_bg(cx);
        let mut header = h_flex()
            .w_full()
            .h(px(36.0))
            .px_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(section_bg);

        let last_index = columns.len().saturating_sub(1);
        for (index, (name, width)) in columns.into_iter().enumerate() {
            let col = div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD);

            if width > 0.0 {
                let col = col.w(px(width));
                if index == last_index {
                    header = header.child(col.text_right().child(name));
                } else {
                    header = header.child(col.child(name));
                }
            } else {
                header = header.child(col.flex_1().child(name));
            }
        }

        header
    }

    /// 渲染 Stream 视图
    fn render_stream_view(
        &self,
        entries: &[crate::StreamEntry],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let muted = cx.theme().muted;
        let accent = cx.theme().accent;
        let muted_foreground = cx.theme().muted_foreground;

        v_flex()
            .id("stream-value-scroll")
            .size_full()
            .p_2()
            .gap_2()
            .overflow_scroll()
            .children(entries.iter().enumerate().map({
                let view = view.clone();
                move |(entry_idx, entry)| {
                    let view = view.clone();

                    v_flex()
                        .w_full()
                        .p_2()
                        .rounded(px(4.0))
                        .bg(muted)
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(accent)
                                .child(format!("ID: {}", entry.id)),
                        )
                        .children(entry.fields.iter().enumerate().map({
                            let view = view.clone();
                            let entry_id = entry.id.clone();
                            move |(field_idx, (k, v))| {
                                let title = large_text_preview_title(
                                    "Stream value",
                                    Some(&format!("{} @ {}", k, entry_id)),
                                );
                                let value = v.clone();
                                let view = view.clone();

                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted_foreground)
                                            .w(px(100.0))
                                            .child(k.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_sm()
                                            .truncate()
                                            .child(v.clone()),
                                    )
                                    .child(
                                        Button::new((
                                            "preview-stream",
                                            ((entry_idx as u64) << 32) | field_idx as u64,
                                        ))
                                        .icon(IconName::Maximize)
                                        .ghost()
                                        .with_size(Size::Medium)
                                        .tooltip(t!("KeyValueView.view_full_value_tooltip"))
                                        .on_click(
                                            move |_, window, cx| {
                                                view.update(cx, |v, cx| {
                                                    v.show_large_text_preview_dialog(
                                                        title.clone(),
                                                        value.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            },
                                        ),
                                    )
                            }
                        }))
                }
            }))
    }

    /// 渲染空状态
    fn render_empty_state(&self, cx: &App) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                Icon::new(IconName::Database.color())
                    .with_size(Size::Large)
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("KeyValueView.select_key_placeholder").to_string()),
            )
    }

    /// 渲染加载状态
    fn render_loading_state(&self, _cx: &App) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(Spinner::new())
    }

    /// 渲染错误状态
    fn render_error_state(&self, error: &str, cx: &App) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                Icon::new(IconName::TriangleAlert)
                    .with_size(Size::Large)
                    .text_color(cx.theme().danger),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(cx.theme().danger)
                    .child(t!("KeyValueView.load_failed", error = error).to_string()),
            )
    }
}

impl EventEmitter<KeyValueViewEvent> for KeyValueView {}

impl Focusable for KeyValueView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for KeyValueView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 应用异步加载后的待设置值
        self.apply_pending_editor_value(window, cx);
        // 筛选/排序结果只在条件变化时重算
        self.refresh_filtered_content();

        v_flex()
            .size_full()
            .when(matches!(self.load_state, LoadState::Empty), |this| {
                this.child(self.render_empty_state(cx))
            })
            .when(matches!(self.load_state, LoadState::Loading), |this| {
                this.child(self.render_loading_state(cx))
            })
            .when(matches!(self.load_state, LoadState::Error(_)), |this| {
                if let LoadState::Error(ref e) = self.load_state {
                    this.child(self.render_error_state(e, cx))
                } else {
                    this
                }
            })
            .when(matches!(self.load_state, LoadState::Loaded), |this| {
                this.child(self.render_key_info(cx))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.render_value_editor(window, cx)),
                    )
                    .child(self.render_status_bar(cx))
            })
    }
}

impl EventEmitter<TabContentEvent> for KeyValueView {}

#[cfg(test)]
mod tests {
    use super::{
        SortOrder, ZSetSortBy, filter_hash_fields, filter_strings, filter_zset_members,
        large_text_preview_title, should_replace_set_member,
    };
    use crate::{HashField, ZSetMember};

    #[test]
    fn filter_strings_空筛选保留全部与原始下标() {
        let items = vec!["alpha".to_string(), "beta".to_string()];

        let filtered = filter_strings(&items, "", false);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0], (0, "alpha".to_string()));
        assert_eq!(filtered[1], (1, "beta".to_string()));
    }

    #[test]
    fn filter_strings_模糊匹配忽略大小写() {
        let items = vec!["Alpha".to_string(), "beta".to_string()];

        let filtered = filter_strings(&items, "alp", false);

        assert_eq!(filtered, vec![(0, "Alpha".to_string())]);
    }

    #[test]
    fn filter_strings_精确匹配区分大小写() {
        let items = vec!["Alpha".to_string(), "beta".to_string()];

        assert!(filter_strings(&items, "alp", true).is_empty());
        assert_eq!(
            filter_strings(&items, "Alpha", true),
            vec![(0, "Alpha".to_string())]
        );
    }

    #[test]
    fn filter_hash_fields_匹配字段与值() {
        let items = vec![
            HashField {
                field: "name".to_string(),
                value: "Alice".to_string(),
            },
            HashField {
                field: "age".to_string(),
                value: "30".to_string(),
            },
        ];

        assert_eq!(filter_hash_fields(&items, "ali", false).len(), 1);
        assert_eq!(filter_hash_fields(&items, "age", false).len(), 1);
        assert!(filter_hash_fields(&items, "bob", false).is_empty());
    }

    #[test]
    fn filter_zset_members_按分数与成员排序() {
        let items = vec![
            ZSetMember {
                member: "b".to_string(),
                score: 2.0,
            },
            ZSetMember {
                member: "a".to_string(),
                score: 1.0,
            },
        ];

        let by_score = filter_zset_members(&items, "", false, ZSetSortBy::Score, SortOrder::Asc);
        assert_eq!(by_score[0].1.member, "a");
        assert_eq!(by_score[1].1.member, "b");

        let by_score_desc =
            filter_zset_members(&items, "", false, ZSetSortBy::Score, SortOrder::Desc);
        assert_eq!(by_score_desc[0].1.member, "b");

        let by_member = filter_zset_members(&items, "", false, ZSetSortBy::Member, SortOrder::Asc);
        assert_eq!(by_member[0].1.member, "a");
        assert_eq!(by_member[0].0, 1, "排序后仍保留原始下标");
    }

    #[test]
    fn large_text_preview_title_formats_context() {
        assert_eq!(
            "List item #1",
            large_text_preview_title("List item", Some("#1"))
        );
    }

    #[test]
    fn large_text_preview_title_omits_blank_context() {
        assert_eq!("Set member", large_text_preview_title("Set member", None));
    }

    #[test]
    fn should_replace_set_member_detects_changes() {
        assert!(should_replace_set_member("old", "new"));
        assert!(!should_replace_set_member("same", "same"));
    }
}

impl TabContent for KeyValueView {
    fn content_key(&self) -> &'static str {
        "KeyValue"
    }

    fn title(&self, _cx: &App) -> SharedString {
        if self.closeable {
            self.current_key
                .clone()
                .unwrap_or_else(|| t!("KeyValueView.tab_title_default").to_string())
                .into()
        } else {
            "Value".into()
        }
    }

    fn icon(&self, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Key).with_size(Size::Medium))
    }

    fn closeable(&self, _cx: &App) -> bool {
        self.closeable
    }

    fn try_close(
        &mut self,
        _tab_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<bool> {
        Task::ready(true)
    }
}
