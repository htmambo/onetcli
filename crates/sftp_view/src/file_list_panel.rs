use gpui::{
    App, Bounds, Context, DragMoveEvent, Empty, Entity, EntityId, FocusHandle, Focusable,
    IntoElement, ListSizingBehavior, MouseButton, MouseDownEvent, ParentElement, Pixels, Render,
    SharedString, Styled, UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use gpui_component::{
    ActiveTheme, ElementExt, Icon, IconName, InteractiveElementExt, Sizable, WindowsSurfaceLayer,
    h_flex,
    input::{Input, InputEvent, InputState},
    layered_surface_color,
    menu::{ContextMenuExt, PopupMenu, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarShow},
    tooltip::Tooltip,
    v_flex,
};
use rust_i18n::t;
use std::collections::HashSet;
use std::ops::Range;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct FileItem {
    pub name: String,
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
    pub permissions: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortColumn {
    Name,
    Modified,
    Size,
    Kind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum FileListColumn {
    Name,
    Modified,
    Size,
    Kind,
    Permissions,
}

#[derive(Clone, Copy, Debug)]
struct FileListColumnWidths {
    name: Pixels,
    modified: Pixels,
    size: Pixels,
    kind: Pixels,
    permissions: Pixels,
}

impl Default for FileListColumnWidths {
    fn default() -> Self {
        Self {
            name: px(220.),
            modified: px(160.),
            size: px(96.),
            kind: px(88.),
            permissions: px(120.),
        }
    }
}

impl FileListColumnWidths {
    fn get(&self, column: FileListColumn) -> Pixels {
        match column {
            FileListColumn::Name => self.name,
            FileListColumn::Modified => self.modified,
            FileListColumn::Size => self.size,
            FileListColumn::Kind => self.kind,
            FileListColumn::Permissions => self.permissions,
        }
    }

    fn set(&mut self, column: FileListColumn, width: Pixels) {
        match column {
            FileListColumn::Name => self.name = width,
            FileListColumn::Modified => self.modified = width,
            FileListColumn::Size => self.size = width,
            FileListColumn::Kind => self.kind = width,
            FileListColumn::Permissions => self.permissions = width,
        }
    }

    fn min_width(column: FileListColumn) -> Pixels {
        match column {
            FileListColumn::Name => px(160.),
            FileListColumn::Modified => px(140.),
            FileListColumn::Size => px(80.),
            FileListColumn::Kind => px(80.),
            FileListColumn::Permissions => px(100.),
        }
    }

    fn max_width(column: FileListColumn) -> Pixels {
        match column {
            FileListColumn::Name => px(480.),
            FileListColumn::Modified => px(280.),
            FileListColumn::Size => px(180.),
            FileListColumn::Kind => px(180.),
            FileListColumn::Permissions => px(240.),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FileListColumnBounds {
    name: Bounds<Pixels>,
    modified: Bounds<Pixels>,
    size: Bounds<Pixels>,
    kind: Bounds<Pixels>,
    permissions: Bounds<Pixels>,
}

impl FileListColumnBounds {
    fn get(&self, column: FileListColumn) -> Bounds<Pixels> {
        match column {
            FileListColumn::Name => self.name,
            FileListColumn::Modified => self.modified,
            FileListColumn::Size => self.size,
            FileListColumn::Kind => self.kind,
            FileListColumn::Permissions => self.permissions,
        }
    }

    fn set(&mut self, column: FileListColumn, bounds: Bounds<Pixels>) {
        match column {
            FileListColumn::Name => self.name = bounds,
            FileListColumn::Modified => self.modified = bounds,
            FileListColumn::Size => self.size = bounds,
            FileListColumn::Kind => self.kind = bounds,
            FileListColumn::Permissions => self.permissions = bounds,
        }
    }
}

#[derive(Clone)]
struct ResizeColumn(pub (EntityId, FileListColumn));

impl Render for ResizeColumn {
    fn render(&mut self, _window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

const FILE_ROW_HEIGHT: Pixels = px(28.);

fn format_file_size(size: u64) -> String {
    if size == 0 {
        return "- -".to_string();
    }
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} kB", size as f64 / KB as f64)
    } else {
        format!("{} {}", size, t!("FileList.bytes"))
    }
}

fn format_modified_time(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = time.into();
    datetime.format("%m/%d/%Y, %I:%M %p").to_string()
}

fn get_file_kind(name: &str) -> String {
    if let Some(ext) = name.rsplit('.').next() {
        if ext != name {
            return ext.to_lowercase();
        }
    }
    t!("FileList.kind_file").to_string()
}

pub struct FileListPanel {
    current_path: String,
    is_remote: bool,

    items: Vec<FileItem>,
    filtered_indices: Vec<usize>,
    selected_indices: HashSet<usize>,
    sort_column: SortColumn,
    sort_order: SortOrder,

    show_hidden: bool,
    search_query: String,
    search_input: Entity<InputState>,

    path_editing: bool,
    path_input: Entity<InputState>,

    scroll_handle: UniformListScrollHandle,
    column_widths: FileListColumnWidths,
    column_bounds: FileListColumnBounds,
    resizing_column: Option<FileListColumn>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl FileListPanel {
    pub fn new(
        initial_path: String,
        is_remote: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("Placeholder.path")));
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("Placeholder.search")));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(
            &path_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.on_path_input_enter(cx);
                }
                InputEvent::Blur => {
                    this.cancel_path_editing(cx);
                }
                _ => {}
            },
        ));

        subscriptions.push(
            cx.subscribe(&search_input, |this, input, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).text().to_string();
                    this.on_search_change(text, cx);
                }
            }),
        );

        Self {
            current_path: initial_path,
            is_remote,
            items: Vec::new(),
            filtered_indices: Vec::new(),
            selected_indices: HashSet::new(),
            sort_column: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            show_hidden: false,
            search_query: String::new(),
            search_input,
            path_editing: false,
            path_input,
            scroll_handle: UniformListScrollHandle::new(),
            column_widths: FileListColumnWidths::default(),
            column_bounds: FileListColumnBounds::default(),
            resizing_column: None,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_items(&mut self, items: Vec<FileItem>, cx: &mut Context<Self>) {
        self.items = items;
        self.selected_indices.clear();
        self.sort_items();
        self.apply_filter();
        cx.notify();
    }

    pub fn set_path(&mut self, path: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_path = path;
        self.path_editing = false;
        self.scroll_handle = UniformListScrollHandle::new();
        cx.notify();
    }

    pub fn set_current_path(&mut self, path: String, cx: &mut Context<Self>) {
        self.current_path = path;
        self.scroll_handle = UniformListScrollHandle::new();
        cx.notify();
    }

    pub fn current_path(&self) -> &str {
        &self.current_path
    }

    fn cancel_path_editing(&mut self, cx: &mut Context<Self>) {
        self.path_editing = false;
        cx.notify();
    }

    fn on_path_input_enter(&mut self, cx: &mut Context<Self>) {
        let new_path = self.path_input.read(cx).text().to_string();
        if !new_path.is_empty() && new_path != self.current_path {
            cx.emit(FileListPanelEvent::PathChanged(new_path));
        }
        self.path_editing = false;
        cx.notify();
    }

    fn on_search_change(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_query = query;
        self.apply_filter();
        self.selected_indices.clear();
        cx.notify();
    }

    fn apply_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        let show_hidden = self.show_hidden;

        self.filtered_indices = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                // 隐藏文件过滤：以 . 开头的文件
                if !show_hidden && item.name.starts_with('.') {
                    return false;
                }
                // 搜索过滤
                if query.is_empty() {
                    true
                } else {
                    item.name.to_lowercase().contains(&query)
                }
            })
            .map(|(i, _)| i)
            .collect();
    }

    /// 切换隐藏文件显示
    pub fn toggle_show_hidden(&mut self, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.apply_filter();
        self.selected_indices.clear();
        cx.notify();
    }

    pub fn selected_items(&self, _cx: &App) -> Vec<FileItem> {
        self.selected_indices
            .iter()
            .filter_map(|&filtered_ix| {
                self.filtered_indices
                    .get(filtered_ix)
                    .and_then(|&real_ix| self.items.get(real_ix).cloned())
            })
            .collect()
    }

    /// 获取用于拖拽的文件项列表
    /// 如果 filtered_ix 在选中列表中，返回所有选中的文件；否则只返回当前文件
    pub fn get_drag_items(&self, filtered_ix: usize) -> Vec<(usize, FileItem)> {
        if self.selected_indices.contains(&filtered_ix) && self.selected_indices.len() > 1 {
            // 当前文件在选中列表中，返回所有选中的文件
            self.selected_indices
                .iter()
                .filter_map(|&idx| {
                    self.filtered_indices
                        .get(idx)
                        .and_then(|&real_ix| self.items.get(real_ix).cloned())
                        .map(|item| (idx, item))
                })
                .collect()
        } else {
            // 当前文件不在选中列表中，只返回当前文件
            self.filtered_indices
                .get(filtered_ix)
                .and_then(|&real_ix| self.items.get(real_ix).cloned())
                .map(|item| vec![(filtered_ix, item)])
                .unwrap_or_default()
        }
    }

    /// 检查某个 filtered_ix 是否在选中列表中
    pub fn is_selected(&self, filtered_ix: usize) -> bool {
        self.selected_indices.contains(&filtered_ix)
    }

    /// 获取选中项的数量
    pub fn selected_count(&self) -> usize {
        self.selected_indices.len()
    }

    pub fn items(&self) -> &[FileItem] {
        &self.items
    }

    fn is_at_root(&self) -> bool {
        if self.is_remote {
            self.current_path == "/" || self.current_path == "." || self.current_path.is_empty()
        } else {
            self.current_path == "/"
                || std::path::Path::new(&self.current_path)
                    .parent()
                    .is_none_or(|path| path.as_os_str().is_empty())
        }
    }

    fn toggle_selection(&mut self, row_ix: usize, multi_select: bool) {
        if multi_select {
            if self.selected_indices.contains(&row_ix) {
                self.selected_indices.remove(&row_ix);
            } else {
                self.selected_indices.insert(row_ix);
            }
        } else {
            if !self.selected_indices.contains(&row_ix) {
                self.selected_indices.clear();
                self.selected_indices.insert(row_ix);
            }
        }
    }

    fn apply_context_selection(selected_indices: &mut HashSet<usize>, row_ix: usize) -> bool {
        if selected_indices.contains(&row_ix) {
            return false;
        }

        selected_indices.clear();
        selected_indices.insert(row_ix);
        true
    }

    fn select_for_context_menu(&mut self, row_ix: usize) -> bool {
        Self::apply_context_selection(&mut self.selected_indices, row_ix)
    }

    fn set_sort(&mut self, column: SortColumn, cx: &mut Context<Self>) {
        if self.sort_column == column {
            self.sort_order = match self.sort_order {
                SortOrder::Ascending => SortOrder::Descending,
                SortOrder::Descending => SortOrder::Ascending,
            };
        } else {
            self.sort_column = column;
            self.sort_order = SortOrder::Ascending;
        }
        self.sort_items();
        self.apply_filter();
        self.selected_indices.clear();
        cx.notify();
    }

    fn sort_items(&mut self) {
        let sort_column = self.sort_column;
        let sort_order = self.sort_order;

        self.items.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                return if a.is_dir {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }

            let cmp = match sort_column {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Modified => a.modified.cmp(&b.modified),
                SortColumn::Size => a.size.cmp(&b.size),
                SortColumn::Kind => get_file_kind(&a.name).cmp(&get_file_kind(&b.name)),
            };

            match sort_order {
                SortOrder::Ascending => cmp,
                SortOrder::Descending => cmp.reverse(),
            }
        });
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let search_input = self.search_input.clone();
        let has_query = !self.search_query.is_empty();
        let filtered_count = self.filtered_indices.len();
        let total_count = self.items.len();
        let blur_enabled = cx.theme().window_blur_enabled;
        let surface_opacity = cx.theme().ui_surface_opacity;
        let search_bg = layered_surface_color(
            cx.theme().background,
            blur_enabled,
            surface_opacity,
            WindowsSurfaceLayer::ContentSection,
        );

        h_flex()
            .h_8()
            .px_2()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(search_bg)
            .child(
                Icon::new(IconName::Search)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div().flex_1().child(
                    Input::new(&search_input)
                        .xsmall()
                        .appearance(false)
                        .cleanable(has_query),
                ),
            )
            .when(has_query, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}/{}", filtered_count, total_count)),
                )
            })
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sort_column = self.sort_column;
        let sort_order = self.sort_order;
        let blur_enabled = cx.theme().window_blur_enabled;
        let surface_opacity = cx.theme().ui_surface_opacity;
        let header_bg = layered_surface_color(
            cx.theme().title_bar,
            blur_enabled,
            surface_opacity,
            WindowsSurfaceLayer::ContentSection,
        );

        h_flex()
            .h_8()
            .px_3()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(header_bg)
            .child(self.render_header_cell(
                t!("FileList.header_name").into(),
                FileListColumn::Name,
                Some(SortColumn::Name),
                sort_column,
                sort_order,
                cx,
            ))
            .child(self.render_header_cell(
                t!("FileList.header_modified").into(),
                FileListColumn::Modified,
                Some(SortColumn::Modified),
                sort_column,
                sort_order,
                cx,
            ))
            .child(self.render_header_cell(
                t!("FileList.header_size").into(),
                FileListColumn::Size,
                Some(SortColumn::Size),
                sort_column,
                sort_order,
                cx,
            ))
            .child(self.render_header_cell(
                t!("FileList.header_kind").into(),
                FileListColumn::Kind,
                Some(SortColumn::Kind),
                sort_column,
                sort_order,
                cx,
            ))
            .child(self.render_header_cell(
                t!("FileList.header_permissions").into(),
                FileListColumn::Permissions,
                None,
                sort_column,
                sort_order,
                cx,
            ))
    }

    fn render_header_cell(
        &self,
        label: SharedString,
        file_column: FileListColumn,
        sort_column: Option<SortColumn>,
        current_sort: SortColumn,
        sort_order: SortOrder,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width = self.column_width(file_column);
        let is_sorted = sort_column.is_some_and(|column| current_sort == column);
        let view = cx.entity();

        h_flex()
            .relative()
            .w(width)
            .min_w(width)
            .max_w(width)
            .flex_shrink_0()
            .h_full()
            .child(
                h_flex()
                    .size_full()
                    .px_2()
                    .items_center()
                    .gap_1()
                    .when(sort_column.is_some(), |this| {
                        this.cursor_pointer()
                            .hover(|style| style.bg(cx.theme().list_active))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _window, cx| {
                                    if let Some(column) = sort_column {
                                        this.set_sort(column, cx);
                                    }
                                }),
                            )
                    })
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .when(is_sorted, |el| {
                        el.child(
                            Icon::new(if sort_order == SortOrder::Ascending {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                        )
                    }),
            )
            .child(self.render_column_resize_handle(file_column, cx))
            .on_prepaint(move |bounds, _, cx| {
                view.update(cx, |this, _| {
                    this.column_bounds.set(file_column, bounds);
                });
            })
    }

    fn render_file_row(
        &self,
        _ix: usize,
        item: &FileItem,
        is_selected: bool,
        cx: &App,
    ) -> impl IntoElement {
        let name = item.name.clone();
        let is_dir = item.is_dir;
        let size = item.size;
        let modified = item.modified;
        let permissions = item.permissions.clone();

        h_flex()
            .w_full()
            .h(FILE_ROW_HEIGHT)
            .px_2()
            .items_center()
            .when(is_selected, |el| el.bg(cx.theme().selection))
            .child(
                h_flex()
                    .w(self.column_width(FileListColumn::Name))
                    .min_w(self.column_width(FileListColumn::Name))
                    .max_w(self.column_width(FileListColumn::Name))
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(if is_dir {
                            IconName::Folder1
                        } else {
                            IconName::File
                        })
                        .small()
                        .color(),
                    )
                    .child({
                        let tooltip_name = name.clone();
                        div().flex_1().overflow_hidden().child(
                            div()
                                .id(SharedString::from(name.clone()))
                                .w_full()
                                .text_base()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(name.clone())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(tooltip_name.clone()).build(window, cx)
                                }),
                        )
                    }),
            )
            .child(self.render_body_text_cell(
                FileListColumn::Modified,
                format_modified_time(modified),
                cx,
            ))
            .child(self.render_body_text_cell(
                FileListColumn::Size,
                if is_dir {
                    "- -".to_string()
                } else {
                    format_file_size(size)
                },
                cx,
            ))
            .child(self.render_body_text_cell(
                FileListColumn::Kind,
                if is_dir {
                    t!("FileList.kind_folder").to_string()
                } else {
                    get_file_kind(&name)
                },
                cx,
            ))
            .child(self.render_body_text_cell(FileListColumn::Permissions, permissions, cx))
    }

    fn render_parent_row(&self, _cx: &App) -> impl IntoElement {
        h_flex()
            .w_full()
            .h(FILE_ROW_HEIGHT)
            .px_2()
            .items_center()
            .child(
                h_flex()
                    .w(self.column_width(FileListColumn::Name))
                    .min_w(self.column_width(FileListColumn::Name))
                    .max_w(self.column_width(FileListColumn::Name))
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::Folder1).small().color())
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_base()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(".."),
                    ),
            )
            .child(self.render_empty_body_cell(FileListColumn::Modified))
            .child(self.render_empty_body_cell(FileListColumn::Size))
            .child(self.render_empty_body_cell(FileListColumn::Kind))
            .child(self.render_empty_body_cell(FileListColumn::Permissions))
    }

    fn column_width(&self, column: FileListColumn) -> Pixels {
        self.column_widths.get(column)
    }

    fn set_column_width(&mut self, column: FileListColumn, width: Pixels) {
        let width = width.clamp(
            FileListColumnWidths::min_width(column),
            FileListColumnWidths::max_width(column),
        );
        self.column_widths.set(column, width);
    }

    fn render_body_text_cell(
        &self,
        column: FileListColumn,
        text: String,
        cx: &App,
    ) -> impl IntoElement {
        let width = self.column_width(column);

        div()
            .w(width)
            .min_w(width)
            .max_w(width)
            .flex_shrink_0()
            .px_2()
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(text),
            )
    }

    fn render_empty_body_cell(&self, column: FileListColumn) -> impl IntoElement {
        let width = self.column_width(column);

        div()
            .w(width)
            .min_w(width)
            .max_w(width)
            .flex_shrink_0()
            .px_2()
    }

    fn render_column_resize_handle(
        &self,
        column: FileListColumn,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        const HANDLE_SIZE: Pixels = px(2.);

        let group_id = SharedString::from(format!("file-list-resize-handle:{column:?}"));
        let is_active = self.resizing_column == Some(column);

        h_flex()
            .id(SharedString::from(format!(
                "file-list-resize-handle-{column:?}"
            )))
            .group(group_id.clone())
            .occlude()
            .cursor_col_resize()
            .h_full()
            .w(HANDLE_SIZE)
            .ml(-HANDLE_SIZE)
            .justify_end()
            .items_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.resizing_column = Some(column);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_drag_move(
                cx.listener(move |this, e: &DragMoveEvent<ResizeColumn>, _window, cx| {
                    match e.drag(cx) {
                        ResizeColumn((entity_id, drag_column)) => {
                            if cx.entity_id() != *entity_id || *drag_column != column {
                                return;
                            }

                            let bounds = this.column_bounds.get(column);
                            let new_width = (e.event.position.x - HANDLE_SIZE - bounds.left())
                                .clamp(
                                    FileListColumnWidths::min_width(column),
                                    FileListColumnWidths::max_width(column),
                                );

                            this.set_column_width(column, new_width);
                            this.resizing_column = Some(column);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }
                }),
            )
            .on_drag(ResizeColumn((cx.entity_id(), column)), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this.resizing_column == Some(column) {
                        this.resizing_column = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this.resizing_column == Some(column) {
                        this.resizing_column = None;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .h_full()
                    .justify_center()
                    .bg(if is_active {
                        cx.theme().drag_border
                    } else {
                        cx.theme().border
                    })
                    .group_hover(&group_id, |this| this.bg(cx.theme().drag_border))
                    .w(px(1.)),
            )
    }

    fn parent_path(current_path: &str, is_remote: bool) -> String {
        if is_remote {
            if current_path == "/" || current_path == "." || current_path.is_empty() {
                return "/".to_string();
            }

            let trimmed = current_path.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(0) | None => "/".to_string(),
                Some(pos) => trimmed[..pos].to_string(),
            }
        } else {
            std::path::Path::new(current_path)
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| current_path.to_string())
        }
    }

    /// 构建文件项的右键菜单
    /// 根据 is_remote（远程/本地）和 is_dir（文件夹/文件）显示不同的菜单项
    fn build_file_context_menu(
        menu: PopupMenu,
        name: &str,
        full_path: &str,
        is_dir: bool,
        is_remote: bool,
        view: &Entity<Self>,
        window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let view_new_file = view.clone();
        let view_new_folder = view.clone();
        let name_for_rename = name.to_string();
        let path_for_rename = full_path.to_string();
        let name_for_download = name.to_string();
        let path_for_download = full_path.to_string();
        let name_for_permissions = name.to_string();
        let path_for_permissions = full_path.to_string();
        let path_for_terminal = full_path.to_string();
        let name_for_copy = name.to_string();
        let path_for_copy = full_path.to_string();
        let name_for_delete = name.to_string();
        let path_for_delete = full_path.to_string();
        let view_toggle_hidden = view.clone();
        let view_refresh = view.clone();
        let view_terminal_current = view.clone();
        let view_rename = view.clone();
        let view_download = view.clone();
        let view_upload = view.clone();
        let view_permissions = view.clone();
        let view_terminal_at = view.clone();
        let view_copy_name = view.clone();
        let view_copy_path = view.clone();
        let view_delete = view.clone();

        let mut menu = menu
            .item(
                PopupMenuItem::new(t!("File.new_file").to_string())
                    .icon(IconName::File)
                    .on_click(window.listener_for(&view_new_file, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::NewFile);
                    })),
            )
            .item(
                PopupMenuItem::new(t!("File.new_folder").to_string())
                    .icon(IconName::NewFolder)
                    .on_click(
                        window.listener_for(&view_new_folder, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::NewFolder);
                        }),
                    ),
            )
            .separator();

        menu = menu.item(
            PopupMenuItem::new(t!("File.rename").to_string())
                .icon(IconName::Edit)
                .on_click(window.listener_for(&view_rename, move |_this, _, _, cx| {
                    cx.emit(FileListPanelEvent::Rename {
                        name: name_for_rename.clone(),
                        full_path: path_for_rename.clone(),
                    });
                })),
        );

        if is_remote {
            menu = menu
                .item(
                    PopupMenuItem::new(t!("Common.download").to_string())
                        .icon(IconName::ArrowDown)
                        .on_click(window.listener_for(&view_download, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::Download {
                                name: name_for_download.clone(),
                                full_path: path_for_download.clone(),
                            });
                        })),
                )
                .item(
                    PopupMenuItem::new(t!("File.change_permission").to_string())
                        .icon(IconName::Key)
                        .on_click(window.listener_for(
                            &view_permissions,
                            move |_this, _, _, cx| {
                                cx.emit(FileListPanelEvent::ChangePermissions {
                                    name: name_for_permissions.clone(),
                                    full_path: path_for_permissions.clone(),
                                });
                            },
                        )),
                );
        } else {
            menu = menu.item(
                PopupMenuItem::new(t!("Common.upload").to_string())
                    .icon(IconName::Upload)
                    .on_click(window.listener_for(&view_upload, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::UploadSelected);
                    })),
            );
        }

        menu = menu.separator();

        if is_dir {
            menu = menu.item(
                PopupMenuItem::new(t!("Terminal.open_here").to_string())
                    .icon(IconName::Terminal)
                    .on_click(
                        window.listener_for(&view_terminal_at, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::OpenInTerminalAt {
                                full_path: path_for_terminal.clone(),
                            });
                        }),
                    ),
            );
        }

        menu = menu.item(
            PopupMenuItem::new(t!("Terminal.open_in_current").to_string())
                .icon(IconName::SquareTerminal)
                .on_click(
                    window.listener_for(&view_terminal_current, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::OpenInTerminal);
                    }),
                ),
        );

        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(t!("File.copy_name").to_string())
                    .icon(IconName::Copy)
                    .on_click(
                        window.listener_for(&view_copy_name, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::CopyFileName {
                                name: name_for_copy.clone(),
                            });
                        }),
                    ),
            )
            .item(
                PopupMenuItem::new(t!("File.copy_path").to_string())
                    .icon(IconName::Copy)
                    .on_click(
                        window.listener_for(&view_copy_path, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::CopyAbsolutePath {
                                full_path: path_for_copy.clone(),
                            });
                        }),
                    ),
            );

        menu = menu.separator().item(
            PopupMenuItem::new(t!("Common.delete").to_string())
                .icon(IconName::Remove)
                .on_click(window.listener_for(&view_delete, move |_this, _, _, cx| {
                    cx.emit(FileListPanelEvent::Delete {
                        name: name_for_delete.clone(),
                        full_path: path_for_delete.clone(),
                    });
                })),
        );

        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(t!("Common.refresh").to_string())
                    .icon(IconName::Refresh)
                    .on_click(window.listener_for(&view_refresh, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::Refresh);
                    })),
            )
            .item(
                PopupMenuItem::new(t!("File.toggle_hidden").to_string())
                    .icon(IconName::Eye)
                    .on_click(
                        window.listener_for(&view_toggle_hidden, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::ToggleHiddenFiles);
                        }),
                    ),
            );

        menu
    }

    /// 构建当前目录空白区域的右键菜单
    fn build_panel_context_menu(
        mut menu: PopupMenu,
        current_path: &str,
        is_remote: bool,
        has_selection: bool,
        view: &Entity<Self>,
        window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let path_for_copy = current_path.to_string();

        let view_new_file = view.clone();
        let view_new_folder = view.clone();
        let view_terminal = view.clone();
        let view_copy_path = view.clone();
        let view_delete = view.clone();
        let view_refresh = view.clone();
        let view_toggle_hidden = view.clone();
        let view_upload = view.clone();
        let view_download = view.clone();
        let can_upload = !is_remote && has_selection;
        let can_download = is_remote && has_selection;
        let can_delete = has_selection;

        menu = menu
            .item(
                PopupMenuItem::new(t!("File.new_file").to_string())
                    .icon(IconName::File)
                    .on_click(window.listener_for(&view_new_file, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::NewFile);
                    })),
            )
            .item(
                PopupMenuItem::new(t!("File.new_folder").to_string())
                    .icon(IconName::NewFolder)
                    .on_click(
                        window.listener_for(&view_new_folder, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::NewFolder);
                        }),
                    ),
            );

        if is_remote {
            if can_download {
                menu = menu.separator().item(
                    PopupMenuItem::new(t!("Common.download").to_string())
                        .icon(IconName::ArrowDown)
                        .on_click(window.listener_for(&view_download, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::Download {
                                name: String::new(),
                                full_path: String::new(),
                            });
                        })),
                );
            }
        } else {
            if can_upload {
                menu = menu.separator().item(
                    PopupMenuItem::new(t!("Common.upload").to_string())
                        .icon(IconName::Upload)
                        .on_click(window.listener_for(&view_upload, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::UploadSelected);
                        })),
                );
            }
        }

        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(t!("Terminal.open_in_current").to_string())
                    .icon(IconName::SquareTerminal)
                    .on_click(window.listener_for(&view_terminal, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::OpenInTerminal);
                    })),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("File.copy_path").to_string())
                    .icon(IconName::Copy)
                    .on_click(
                        window.listener_for(&view_copy_path, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::CopyAbsolutePath {
                                full_path: path_for_copy.clone(),
                            });
                        }),
                    ),
            );

        if can_delete {
            menu = menu.separator().item(
                PopupMenuItem::new(t!("Common.delete").to_string())
                    .icon(IconName::Remove)
                    .on_click(window.listener_for(&view_delete, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::Delete {
                            name: String::new(),
                            full_path: String::new(),
                        });
                    })),
            );
        }

        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(t!("Common.refresh").to_string())
                    .icon(IconName::Refresh)
                    .on_click(window.listener_for(&view_refresh, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::Refresh);
                    })),
            )
            .item(
                PopupMenuItem::new(t!("File.toggle_hidden").to_string())
                    .icon(IconName::Eye)
                    .on_click(
                        window.listener_for(&view_toggle_hidden, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::ToggleHiddenFiles);
                        }),
                    ),
            );

        menu
    }

    /// 构建上级目录行（..）的右键菜单
    fn build_parent_context_menu(
        mut menu: PopupMenu,
        parent_path: &str,
        view: &Entity<Self>,
        window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let path_for_terminal = parent_path.to_string();
        let path_for_copy = parent_path.to_string();

        let view_go_parent = view.clone();
        let view_terminal = view.clone();
        let view_copy_path = view.clone();
        let view_refresh = view.clone();

        menu = menu
            .item(
                PopupMenuItem::new(t!("File.go_parent").to_string())
                    .icon(IconName::ArrowUp)
                    .on_click(
                        window.listener_for(&view_go_parent, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::ItemDoubleClicked("..".to_string()));
                        }),
                    ),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("Terminal.open_here").to_string())
                    .icon(IconName::Terminal)
                    .on_click(window.listener_for(&view_terminal, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::OpenInTerminalAt {
                            full_path: path_for_terminal.clone(),
                        });
                    })),
            )
            .item(
                PopupMenuItem::new(t!("File.copy_path").to_string())
                    .icon(IconName::Copy)
                    .on_click(
                        window.listener_for(&view_copy_path, move |_this, _, _, cx| {
                            cx.emit(FileListPanelEvent::CopyAbsolutePath {
                                full_path: path_for_copy.clone(),
                            });
                        }),
                    ),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("Common.refresh").to_string())
                    .icon(IconName::Refresh)
                    .on_click(window.listener_for(&view_refresh, move |_this, _, _, cx| {
                        cx.emit(FileListPanelEvent::Refresh);
                    })),
            );

        menu
    }
}

#[derive(Clone, Debug)]
pub enum FileListPanelEvent {
    PathChanged(String),
    ItemDoubleClicked(String),
    SelectionChanged(Vec<String>),
    /// 新建文件
    NewFile,
    /// 新建文件夹
    NewFolder,
    /// 重命名文件/文件夹
    Rename {
        name: String,
        full_path: String,
    },
    /// 下载文件/文件夹
    Download {
        name: String,
        full_path: String,
    },
    /// 修改权限
    ChangePermissions {
        name: String,
        full_path: String,
    },
    /// 在终端中打开当前目录
    OpenInTerminal,
    /// 在终端中打开到文件/文件夹
    OpenInTerminalAt {
        full_path: String,
    },
    /// 复制文件名
    CopyFileName {
        name: String,
    },
    /// 复制绝对路径
    CopyAbsolutePath {
        full_path: String,
    },
    /// 删除文件/文件夹
    Delete {
        name: String,
        full_path: String,
    },
    /// 上传当前选择的文件或文件夹
    UploadSelected,
    /// 刷新列表
    Refresh,
    /// 显示隐藏文件
    ToggleHiddenFiles,
}

#[derive(Clone, Debug)]
pub struct DraggedFileItem {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
    pub is_remote: bool,
}

/// 支持多文件拖拽的结构体
#[derive(Clone, Debug)]
pub struct DraggedFileItems {
    pub items: Vec<DraggedFileItem>,
    pub is_remote: bool,
}

impl DraggedFileItems {
    pub fn single(item: DraggedFileItem) -> Self {
        let is_remote = item.is_remote;
        Self {
            items: vec![item],
            is_remote,
        }
    }

    pub fn multiple(items: Vec<DraggedFileItem>, is_remote: bool) -> Self {
        Self { items, is_remote }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Render for DraggedFileItems {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.items.len();
        let blur_enabled = cx.theme().window_blur_enabled;
        let surface_opacity = cx.theme().ui_surface_opacity;
        let drag_card_bg = layered_surface_color(
            cx.theme().background,
            blur_enabled,
            surface_opacity,
            WindowsSurfaceLayer::ContentCard,
        );

        if count == 1 {
            // 单个文件显示详细信息
            let item = &self.items[0];
            h_flex()
                .id("dragged-file-items")
                .cursor_grab()
                .py_1()
                .px_3()
                .gap_2()
                .items_center()
                .bg(drag_card_bg)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .shadow_md()
                .child(
                    Icon::new(if item.is_dir {
                        IconName::Folder
                    } else {
                        IconName::File
                    })
                    .text_color(if item.is_dir {
                        cx.theme().link
                    } else {
                        cx.theme().muted_foreground
                    }),
                )
                .child(div().text_sm().child(item.name.clone()))
                .into_any_element()
        } else {
            // 多个文件显示数量
            h_flex()
                .id("dragged-file-items")
                .cursor_grab()
                .py_1()
                .px_3()
                .gap_2()
                .items_center()
                .bg(drag_card_bg)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .shadow_md()
                .child(Icon::new(IconName::Folder1).text_color(cx.theme().link))
                .child(
                    div()
                        .text_sm()
                        .child(t!("File.items_count", count = count).to_string()),
                )
                .into_any_element()
        }
    }
}

impl Render for DraggedFileItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blur_enabled = cx.theme().window_blur_enabled;
        let surface_opacity = cx.theme().ui_surface_opacity;
        let drag_card_bg = layered_surface_color(
            cx.theme().background,
            blur_enabled,
            surface_opacity,
            WindowsSurfaceLayer::ContentCard,
        );

        h_flex()
            .id("dragged-file-item")
            .cursor_grab()
            .py_1()
            .px_3()
            .gap_2()
            .items_center()
            .bg(drag_card_bg)
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .shadow_md()
            .child(
                Icon::new(if self.is_dir {
                    IconName::Folder
                } else {
                    IconName::File
                })
                .text_color(if self.is_dir {
                    cx.theme().link
                } else {
                    cx.theme().muted_foreground
                }),
            )
            .child(div().text_sm().child(self.name.clone()))
    }
}

impl gpui::EventEmitter<FileListPanelEvent> for FileListPanel {}

impl Focusable for FileListPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FileListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_count = self.filtered_indices.len();
        let show_parent = !self.is_at_root();
        let total_count = if show_parent {
            filtered_count + 1
        } else {
            filtered_count
        };
        let has_selection_for_menu = !self.selected_indices.is_empty();
        let current_path_for_menu = self.current_path.clone();
        let is_remote_for_menu = self.is_remote;
        let view_for_menu = cx.entity();

        v_flex()
            .size_full()
            .child(self.render_search_bar(cx))
            .child(self.render_header(cx))
            .child(
                div()
                    .flex_1()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .context_menu(move |menu, window, cx| {
                                Self::build_panel_context_menu(
                                    menu,
                                    &current_path_for_menu,
                                    is_remote_for_menu,
                                    has_selection_for_menu,
                                    &view_for_menu,
                                    window,
                                    cx,
                                )
                            }),
                    )
                    .child(
                        uniform_list("file-list", total_count, {
                            cx.processor(move |state: &mut Self, range: Range<usize>, _window, cx| {
                                let current_path = state.current_path.clone();
                                let is_remote = state.is_remote;
                                let has_parent = !state.is_at_root();
                                let parent_path = Self::parent_path(&current_path, is_remote);
                                let view = cx.entity();
                                range
                                    .map(|list_ix| {
                                        if has_parent && list_ix == 0 {
                                            let parent_path_for_menu = parent_path.clone();
                                            let parent_view = view.clone();
                                            return div()
                                                .id(list_ix)
                                                .w_full()
                                                .cursor_pointer()
                                                .block_mouse_except_scroll()
                                                .hover(|style| style.bg(cx.theme().list_hover))
                                                .on_double_click(cx.listener(
                                                    move |_this, _, _window, cx| {
                                                        cx.emit(FileListPanelEvent::ItemDoubleClicked(
                                                            "..".to_string(),
                                                        ));
                                                    },
                                                ))
                                                .context_menu(move |menu, window, cx| {
                                                    Self::build_parent_context_menu(
                                                        menu,
                                                        &parent_path_for_menu,
                                                        &parent_view,
                                                        window,
                                                        cx,
                                                    )
                                                })
                                                .child(state.render_parent_row(cx))
                                                .into_any_element();
                                        }

                                        let filtered_ix = if has_parent { list_ix - 1 } else { list_ix };
                                        let real_ix = state.filtered_indices[filtered_ix];
                                        let item = &state.items[real_ix];
                                        let is_selected = state.selected_indices.contains(&filtered_ix);
                                        let item_name = item.name.clone();
                                        let is_dir = item.is_dir;
                                        let full_path = if is_remote {
                                            if current_path.ends_with('/') {
                                                format!("{}{}", current_path, item_name)
                                            } else {
                                                format!("{}/{}", current_path, item_name)
                                            }
                                        } else {
                                            std::path::Path::new(&current_path)
                                                .join(&item_name)
                                                .to_string_lossy()
                                                .to_string()
                                        };

                                        // 构建拖拽项目列表
                                        // 如果当前文件在选中列表中且有多个选中项，则拖拽所有选中项
                                        // 否则只拖拽当前文件
                                        let drag_items = if state.selected_indices.contains(&filtered_ix)
                                            && state.selected_indices.len() > 1
                                        {
                                            // 拖拽所有选中的文件
                                            let items: Vec<DraggedFileItem> = state
                                                .selected_indices
                                                .iter()
                                                .filter_map(|&idx| {
                                                    state.filtered_indices.get(idx).and_then(|&real_ix| {
                                                        state.items.get(real_ix).map(|item| {
                                                            let item_path = if is_remote {
                                                                if current_path.ends_with('/') {
                                                                    format!("{}{}", current_path, item.name)
                                                                } else {
                                                                    format!(
                                                                        "{}/{}",
                                                                        current_path, item.name
                                                                    )
                                                                }
                                                            } else {
                                                                std::path::Path::new(&current_path)
                                                                    .join(&item.name)
                                                                    .to_string_lossy()
                                                                    .to_string()
                                                            };
                                                            DraggedFileItem {
                                                                name: item.name.clone(),
                                                                is_dir: item.is_dir,
                                                                full_path: item_path,
                                                                is_remote,
                                                            }
                                                        })
                                                    })
                                                })
                                                .collect();
                                            DraggedFileItems::multiple(items, is_remote)
                                        } else {
                                            // 只拖拽当前文件
                                            DraggedFileItems::single(DraggedFileItem {
                                                name: item_name.clone(),
                                                is_dir,
                                                full_path: full_path.clone(),
                                                is_remote,
                                            })
                                        };

                                        // 右键菜单需要的变量
                                        let ctx_name = item_name.clone();
                                        let ctx_full_path = full_path.clone();
                                        let ctx_is_dir = is_dir;
                                        let ctx_is_remote = is_remote;
                                        let ctx_view = view.clone();

                                        div()
                                            .id(list_ix)
                                            .w_full()
                                            .cursor_pointer()
                                            .block_mouse_except_scroll()
                                            .when(!is_selected, |el| {
                                                el.hover(|style| style.bg(cx.theme().list_hover))
                                            })
                                            .on_drag(drag_items, |drag, _, _, cx| cx.new(|_| drag.clone()))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, event: &MouseDownEvent, _window, cx| {
                                                        let multi_select = event.modifiers.secondary();
                                                        this.toggle_selection(filtered_ix, multi_select);
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                cx.listener(move |this, _, _window, cx| {
                                                    if this.select_for_context_menu(filtered_ix) {
                                                        cx.notify();
                                                    }
                                                }),
                                            )
                                            .on_double_click(cx.listener({
                                                let name = item_name.clone();
                                                move |_this, _, _window, cx| {
                                                    if is_dir {
                                                        cx.emit(FileListPanelEvent::ItemDoubleClicked(
                                                            name.clone(),
                                                        ));
                                                    }
                                                }
                                            }))
                                            .context_menu(move |menu, window, cx| {
                                                Self::build_file_context_menu(
                                                    menu,
                                                    &ctx_name,
                                                    &ctx_full_path,
                                                    ctx_is_dir,
                                                    ctx_is_remote,
                                                    &ctx_view,
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .child(state.render_file_row(
                                                filtered_ix,
                                                item,
                                                is_selected,
                                                cx,
                                            ))
                                            .into_any_element()
                                    })
                                    .collect()
                            })
                        })
                        .flex_1()
                        .size_full()
                        .track_scroll(&self.scroll_handle)
                        .with_sizing_behavior(ListSizingBehavior::Auto),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(Scrollbar::width())
                            .child(
                                Scrollbar::vertical(&self.scroll_handle)
                                    .scrollbar_show(ScrollbarShow::Always),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::FileListPanel;
    use std::collections::HashSet;

    #[test]
    fn parent_path_returns_remote_parent_directory() {
        assert_eq!(
            FileListPanel::parent_path("/root/projects/demo", true),
            "/root/projects"
        );
        assert_eq!(
            FileListPanel::parent_path("/root/projects/demo/", true),
            "/root/projects"
        );
    }

    #[test]
    fn parent_path_keeps_remote_root_stable() {
        assert_eq!(FileListPanel::parent_path("/", true), "/");
        assert_eq!(FileListPanel::parent_path(".", true), "/");
        assert_eq!(FileListPanel::parent_path("", true), "/");
    }

    #[test]
    fn parent_path_returns_local_parent_directory() {
        assert_eq!(
            FileListPanel::parent_path("workspace/project", false),
            "workspace"
        );
    }

    #[test]
    fn parent_path_keeps_local_rootless_path_stable() {
        assert_eq!(FileListPanel::parent_path("workspace", false), "workspace");
    }

    #[test]
    fn context_selection_keeps_existing_multi_selection_when_row_already_selected() {
        let mut selected = HashSet::from([1usize, 3usize]);

        let changed = FileListPanel::apply_context_selection(&mut selected, 3);

        assert!(!changed);
        assert_eq!(selected, HashSet::from([1usize, 3usize]));
    }

    #[test]
    fn context_selection_replaces_old_selection_when_row_was_not_selected() {
        let mut selected = HashSet::from([1usize, 3usize]);

        let changed = FileListPanel::apply_context_selection(&mut selected, 2);

        assert!(changed);
        assert_eq!(selected, HashSet::from([2usize]));
    }
}
