use std::collections::HashSet;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AsyncApp, ClickEvent, Context, Corner, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, PathPromptOptions, SharedString, Styled, Subscription,
    Window, actions, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, Size, WindowExt,
    WindowsSurfaceLayer, button::Button, h_flex, layered_level_surface_color, v_flex,
};
use one_ui::edit_table::{Column, EditTable, EditTableEvent, EditTableState};
use one_ui::{create_large_text_editor_with_content, large_text_values_equivalent};
use rust_i18n::t;
use rust_xlsxwriter::Workbook;
use tracing::{error, log::trace};

use crate::import_export::table_export_view::DataExportView;
use crate::settings::{LargeTextEditorOpenMode, current_settings as current_db_view_settings};
use crate::sql_editor::SqlEditor;
use crate::table_data::copy_format::{CopyFormat, CopyFormatter, TableMetadata};
use crate::search_shortcut::{DB_SEARCH_CONTEXT, FocusSearchInput, focus_search_input};
use crate::table_data::filter_editor::{FilterEditorEvent, TableFilterEditor, TableSchema};
use crate::table_data::results_delegate::{EditorTableDelegate, RowChange};
use chrono::Local;
use db::{
    ColumnInfo, ExecOptions, GlobalDbState, IndexInfo, QueryResult, SqlResult, TableCellChange,
    TableDataRequest, TableRowChange, TableSaveRequest,
};
use db::ipc::EXTERNAL_DRIVER_ID_PARAM;
use gpui_component::button::ButtonVariants;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use one_core::popup_window::{PopupWindowOptions, open_popup_window};
use one_core::tab_container::TabContainer;
use one_core::storage::DatabaseType;
use one_ui::edit_table::ColumnSort;
use std::path::PathBuf;

#[cfg(test)]
use db::DbManager;
#[cfg(test)]
use one_core::storage::DatabaseType;

actions!(data_grid, [Page100, Page200, Page300, Page500, Page1000]);

#[cfg(test)]
fn build_header_order_by_clause(
    db_manager: &DbManager,
    database_type: DatabaseType,
    column_name: &str,
    sort: ColumnSort,
) -> Result<Option<String>, String> {
    let direction = match sort {
        ColumnSort::Ascending => "ASC",
        ColumnSort::Descending => "DESC",
        ColumnSort::Default => return Ok(None),
    };

    let plugin = db_manager
        .get_plugin(&database_type)
        .map_err(|_| t!("TableDataGrid.plugin_unavailable").to_string())?;

    Ok(Some(format!(
        "{} {}",
        plugin.quote_identifier(column_name),
        direction
    )))
}

fn collect_delete_row_indices<I>(selected_rows: I, fallback_row: Option<usize>) -> Vec<usize>
where
    I: IntoIterator<Item = usize>,
{
    let mut row_indices: Vec<usize> = selected_rows.into_iter().collect();

    if row_indices.is_empty() {
        if let Some(row_ix) = fallback_row {
            row_indices.push(row_ix);
        }
    } else {
        row_indices.sort_unstable();
        row_indices.dedup();
    }

    row_indices.sort_unstable_by(|left, right| right.cmp(left));
    row_indices
}

fn build_large_text_editor_title(column_name: &str, display_row_ix: usize) -> String {
    t!(
        "TableDataGrid.edit_cell_title",
        column = column_name,
        row = display_row_ix + 1
    )
    .to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LargeTextCellTarget {
    pub display_row_ix: usize,
    pub actual_row_ix: usize,
    pub col_ix: usize,
    pub column_name: String,
    pub value: String,
    pub editable: bool,
}

#[derive(Clone, Debug)]
pub enum DataGridEvent {
    LargeTextSelectionChanged,
    ToggleLargeTextEditorRequested,
    OpenTableDesignerRequested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LargeTextEditorRoute {
    PagePreview,
    Dialog,
}

fn resolve_large_text_editor_route(mode: LargeTextEditorOpenMode) -> LargeTextEditorRoute {
    match mode {
        LargeTextEditorOpenMode::SidebarPreview => LargeTextEditorRoute::PagePreview,
        LargeTextEditorOpenMode::Dialog => LargeTextEditorRoute::Dialog,
    }
}

/// 数据表格使用场景
#[derive(Clone, Debug, PartialEq)]
pub enum DataGridUsage {
    /// 在表格数据页签中使用（编辑器高度较低）
    TableData,
    /// 在SQL结果页签中使用（编辑器高度较高）
    SqlResult,
}

/// 数据表格配置
#[derive(Clone, Debug, PartialEq)]
pub struct DataGridConfig {
    /// 数据库名称
    pub database_name: String,
    /// Schema名称（用于支持schema的数据库如PostgreSQL、MSSQL、Oracle）
    pub schema_name: Option<String>,
    /// 表名称
    pub table_name: String,
    /// 数据库连接ID
    pub connection_id: String,
    /// 数据库类型
    pub database_type: one_core::storage::DatabaseType,
    /// 是否允许编辑
    pub editable: bool,
    /// 是否显示工具栏
    pub show_toolbar: bool,
    /// 使用场景
    pub usage: DataGridUsage,
    /// 原始 SQL（SqlResult 场景使用）
    pub sql: String,
    /// 执行时间（SqlResult 场景使用）
    execution_time: u128,
    /// 数据行数（SqlResult 场景使用）
    rows_count: usize,
    /// 逐步撤销栈容量（0表示禁用）
    undo_stack_size: usize,
}

impl DataGridConfig {
    pub fn new(
        database_name: impl Into<String>,
        table_name: impl Into<String>,
        connection_id: impl Into<String>,
        database_type: one_core::storage::DatabaseType,
    ) -> Self {
        Self {
            database_name: database_name.into(),
            schema_name: None,
            table_name: table_name.into(),
            connection_id: connection_id.into(),
            database_type,
            editable: true,
            show_toolbar: true,
            usage: DataGridUsage::TableData,
            sql: "".to_string(),
            execution_time: 0,
            rows_count: 0,
            undo_stack_size: 50,
        }
    }

    pub fn with_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema_name = Some(schema.into());
        self
    }

    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    pub fn show_toolbar(mut self, show: bool) -> Self {
        self.show_toolbar = show;
        self
    }

    pub fn usage(mut self, usage: DataGridUsage) -> Self {
        self.usage = usage;
        self
    }

    pub fn sql(mut self, sql: impl Into<String>) -> Self {
        self.sql = sql.into();
        self
    }
    pub fn execution_time(mut self, execution_time: u128) -> Self {
        self.execution_time = execution_time;
        self
    }
    pub fn rows_count(mut self, rows_count: usize) -> Self {
        self.rows_count = rows_count;
        self
    }
    pub fn undo_stack_size(mut self, size: usize) -> Self {
        self.undo_stack_size = size;
        self
    }
}

#[derive(Clone)]
pub struct TableDataInfo {
    pub current_page: usize,
    pub page_size: usize,
    pub total_count: usize,
    pub duration: u128,
    pub current_sql: String,
    pub columns: Vec<ColumnInfo>,
    pub index_infos: Vec<IndexInfo>,
    pub error_message: Option<String>,
}

impl Default for TableDataInfo {
    fn default() -> Self {
        Self {
            current_page: 1,
            page_size: 500,
            total_count: 0,
            duration: 0,
            current_sql: String::new(),
            columns: vec![],
            index_infos: vec![],
            error_message: None,
        }
    }
}

#[derive(Clone, Copy)]
enum ExportScope {
    All,
    CurrentPage,
}

#[derive(Clone, Copy)]
enum ExportFormat {
    Xlsx,
    Csv,
    InsertSql,
}

#[derive(Clone)]
enum ReloadAction {
    ApplyFilters,
    ApplySort {
        column_name: String,
        sort: ColumnSort,
    },
    RefreshCurrent,
    LoadPage(usize),
    ChangePageSize(usize),
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Csv => "csv",
            ExportFormat::InsertSql => "sql",
        }
    }

    fn copy_format(self) -> CopyFormat {
        match self {
            ExportFormat::Xlsx | ExportFormat::Csv => CopyFormat::Csv,
            ExportFormat::InsertSql => CopyFormat::SqlInsert,
        }
    }

    fn include_header(self) -> bool {
        matches!(self, ExportFormat::Xlsx | ExportFormat::Csv)
    }
}

fn build_export_bytes(
    format: ExportFormat,
    rows: Vec<Vec<Option<String>>>,
    columns: Vec<SharedString>,
    mut metadata: TableMetadata,
) -> Result<Option<Vec<u8>>, String> {
    if rows.is_empty() {
        return Ok(None);
    }

    let mut export_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|value| value.unwrap_or_else(|| "NULL".to_string()))
                .collect()
        })
        .collect();

    if format.include_header() {
        let header = columns
            .iter()
            .map(|column| column.as_ref().to_string())
            .collect();
        export_rows.insert(0, header);
    }

    metadata.column_names = columns.clone();

    match format {
        ExportFormat::Xlsx => build_xlsx_bytes(&export_rows).map(Some),
        _ => Ok(Some(
            CopyFormatter::format(format.copy_format(), &export_rows, &columns, &metadata)
                .into_bytes(),
        )),
    }
}

fn build_xlsx_bytes(rows: &[Vec<String>]) -> Result<Vec<u8>, String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    for (row_index, row) in rows.iter().enumerate() {
        for (col_index, cell) in row.iter().enumerate() {
            worksheet
                .write_string(row_index as u32, col_index as u16, cell)
                .map_err(|error| error.to_string())?;
        }
    }

    workbook.save_to_buffer().map_err(|error| error.to_string())
}

/// 数据表格组件
pub struct DataGrid {
    /// 组件配置
    config: DataGridConfig,
    /// 内部表格状态
    pub(crate) table: Entity<EditTableState<EditorTableDelegate>>,
    /// 表格事件订阅
    _table_sub: Option<Subscription>,
    /// 焦点句柄
    focus_handle: FocusHandle,
    /// 表格数据信息（分页、总数等）
    table_data_info: Entity<TableDataInfo>,
    /// 过滤器编辑器
    filter_editor: Entity<TableFilterEditor>,
    /// 过滤器事件订阅
    _filter_sub: Option<Subscription>,
    /// 当前页本地搜索
    search_input: Entity<InputState>,
    /// 搜索框事件订阅
    _search_sub: Option<Subscription>,
    /// 隐藏的列（本地持久化）
    hidden_columns: HashSet<SharedString>,
    /// 列可见性是否已加载
    column_visibility_loaded: bool,
    /// 侧边栏大文本编辑器是否已为当前表格打开
    is_large_text_editor_sidebar_open: bool,
    /// 自身实体弱引用，用于异步回调中更新状态
    self_entity: Option<gpui::WeakEntity<Self>>,
}

impl DataGrid {
    pub fn new(config: DataGridConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editable = config.editable;
        let is_table_data = config.usage == DataGridUsage::TableData;
        let database_type = config.database_type;
        let table_name = config.table_name.clone();
        let data_grid_handle = cx.entity().downgrade();
        let table = cx.new(|cx| {
            let mut delegate =
                EditorTableDelegate::new(vec![], vec![], editable, database_type, window, cx);
            // 设置表名，用于 SQL 生成
            delegate.set_table_name(table_name);
            EditTableState::new(delegate, window, cx)
        });
        // 根据应用字号动态计算行高：font_size * 2 + 2（18=>38px，16=>34px，14=>30px）
        let font_size = cx.theme().font_size;
        let row_height = font_size * 2.0 + px(2.);
        table.update(cx, |state, _| {
            let delegate = state.delegate_mut();
            delegate.set_data_grid(data_grid_handle.clone());
            delegate.set_undo_stack_size(config.undo_stack_size);
            state.set_row_height(row_height);
        });
        let focus_handle = cx.focus_handle();
        let filter_editor = cx.new(|cx| TableFilterEditor::new(window, cx));
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("TableDataGrid.search_placeholder").to_string())
                .clean_on_escape()
        });
        let table_data_info = cx.new(|_| TableDataInfo::default());

        let mut result = Self {
            config,
            table,
            _table_sub: None,
            focus_handle,
            table_data_info,
            filter_editor,
            _filter_sub: None,
            search_input,
            _search_sub: None,
            hidden_columns: HashSet::new(),
            column_visibility_loaded: false,
            is_large_text_editor_sidebar_open: false,
            self_entity: Some(data_grid_handle),
        };
        result.bind_table_event(window, cx);
        if is_table_data {
            result.bind_filter_event(window, cx);
            result.bind_search_event(window, cx);
            result.load_data_with_clauses(1, cx);
        }
        result
    }

    fn bind_table_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(
            &self.table,
            window,
            |_this, _, evt: &EditTableEvent, _window, cx| match evt {
                EditTableEvent::SelectCell(row, col) => {
                    trace!("select cell: {:?}", (row, col));
                    cx.emit(DataGridEvent::LargeTextSelectionChanged);
                }
                EditTableEvent::SelectionChanged(_) | EditTableEvent::CellEdited(_, _) => {
                    cx.emit(DataGridEvent::LargeTextSelectionChanged);
                }
                _ => {}
            },
        );
        self._table_sub = Some(sub);
    }

    fn bind_filter_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(
            &self.filter_editor,
            window,
            |this: &mut DataGrid, _, evt: &FilterEditorEvent, window, cx| {
                if matches!(evt, FilterEditorEvent::QueryApply) {
                    this.request_reload_action(ReloadAction::ApplyFilters, window, cx);
                    cx.notify();
                }
            },
        );
        self._filter_sub = Some(sub);
    }

    fn bind_search_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(
            &self.search_input,
            window,
            |this: &mut DataGrid, input, evt: &InputEvent, _window, cx| {
                if let InputEvent::Change = evt {
                    let query = input.read(cx).text().to_string();
                    this.apply_row_search_query(query, cx);
                }
            },
        );
        self._search_sub = Some(sub);
    }

    fn apply_row_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_row_search_query(query);
            cx.notify();
        });
        cx.notify();
    }

    fn on_action_focus_search(
        &mut self,
        _: &FocusSearchInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config.usage == DataGridUsage::TableData {
            focus_search_input(&self.search_input, window, cx);
        }
    }


    fn apply_column_visibility(&self, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            state
                .delegate_mut()
                .update_visible_columns(&self.hidden_columns);
            state.refresh(cx);
        });
    }

    fn column_visibility_key(&self) -> String {
        format!(
            "column_visibility:{}:{}:{}",
            self.config.connection_id, self.config.database_name, self.config.table_name
        )
    }

    fn save_column_visibility(&self, cx: &mut App) {
        let storage = cx.try_global::<one_core::storage::GlobalStorageState>();
        let Some(storage) = storage else { return };
        let Some(kv_repo) = storage
            .storage
            .get::<one_core::storage::KeyValueRepository>()
        else {
            return;
        };
        let key = self.column_visibility_key();
        let hidden: Vec<String> = self.hidden_columns.iter().map(|s| s.to_string()).collect();
        if let Ok(value) = serde_json::to_string(&hidden) {
            let _ = kv_repo.set(&key, &value);
        }
    }

    // ========== 公共访问器 ==========

    pub fn table(&self) -> &Entity<EditTableState<EditorTableDelegate>> {
        &self.table
    }

    pub fn update_data(
        &self,
        columns: Vec<Column>,
        rows: Vec<Vec<Option<String>>>,
        rowids: Vec<String>,
        cx: &mut App,
    ) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().update_data(columns, rows, rowids, cx);
            state.refresh(cx);
        });
    }

    pub fn load_column_meta_if_editable(&self, cx: &mut App) {
        if self.config.usage != DataGridUsage::SqlResult
            || self.config.sql.is_empty()
            || !self.config.editable
        {
            return;
        }

        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();
        let table = self.table.clone();
        let table_info = self.table_data_info.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let result = global_state
                .list_columns(cx, connection_id, database_name, schema_name, table_name)
                .await;

            if let Ok(cols) = result {
                cx.update(|cx| {
                    table_info.update(cx, |info, _cx| {
                        info.columns = cols.clone();
                    });
                    table.update(cx, |state, cx| {
                        state.delegate_mut().set_column_meta(cols);
                        cx.notify();
                    });
                })
            }
        })
        .detach();
    }

    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if self.config.editable == editable {
            return;
        }

        self.config.editable = editable;
        self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            delegate.set_editable(editable);
            if !editable {
                delegate.clear_changes();
            }
            state.refresh(cx);
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn apply_column_sort(
        &mut self,
        column_name: &str,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.config.usage != DataGridUsage::TableData {
            return;
        }

        tracing::info!(
            "[SORT] apply_column_sort: column={}, sort={:?}",
            column_name,
            sort
        );

        self.request_reload_action(
            ReloadAction::ApplySort {
                column_name: column_name.to_string(),
                sort,
            },
            window,
            cx,
        );
    }

    fn commit_active_cell_edit(&self, window: &mut Window, cx: &mut App) {
        // 先提交当前编辑中的单元格，避免未提交输入绕过未保存检查。
        self.table.update(cx, |state, cx| {
            state.commit_cell_edit(window, cx);
        });
    }

    fn request_reload_action(&self, action: ReloadAction, window: &mut Window, cx: &mut App) {
        self.commit_active_cell_edit(window, cx);

        if self.config.usage != DataGridUsage::TableData || !self.has_unsaved_changes(cx) {
            self.execute_reload_action(action, cx);
            return;
        }

        self.confirm_discard_and_reload(action, window, cx);
    }

    fn confirm_discard_and_reload(&self, action: ReloadAction, window: &mut Window, cx: &mut App) {
        let data_grid = self.clone();
        let title = format!(
            "{} {}.{}",
            t!("Common.refresh"),
            self.config.database_name,
            self.config.table_name
        );

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let data_grid = data_grid.clone();
            let action = action.clone();

            dialog
                .title(title.clone())
                .overlay_closable(false)
                .close_button(true)
                .footer(move |_ok, _cancel, _window, _cx| {
                    let data_grid = data_grid.clone();
                    let action = action.clone();

                    vec![
                        Button::new("cancel")
                            .label(t!("Common.cancel").to_string())
                            .on_click(move |_, window: &mut Window, cx| {
                                window.close_dialog(cx);
                            })
                            .into_any_element(),
                        Button::new("discard")
                            .label(t!("Common.discard").to_string())
                            .on_click(move |_, window: &mut Window, cx| {
                                window.close_dialog(cx);
                                data_grid.revert_changes(cx);
                                data_grid.execute_reload_action(action.clone(), cx);
                            })
                            .into_any_element(),
                    ]
                })
                .child(t!("Table.unsaved_changes_prompt").to_string())
        });
    }

    fn execute_reload_action(&self, action: ReloadAction, cx: &mut App) {
        match action {
            ReloadAction::ApplyFilters => self.load_data_with_clauses(1, cx),
            ReloadAction::ApplySort { column_name, sort } => {
                self.filter_editor.update(cx, |editor, cx| {
                    editor.add_sort_column(&column_name, sort, cx);
                });
                self.load_data_with_clauses(1, cx);
            }
            ReloadAction::RefreshCurrent => self.refresh_data(cx),
            ReloadAction::LoadPage(page) => self.load_data_with_clauses(page, cx),
            ReloadAction::ChangePageSize(new_size) => {
                self.table_data_info.update(cx, |info, cx| {
                    info.page_size = new_size;
                    cx.notify();
                });
                self.load_data_with_clauses(1, cx);
            }
        }
    }

    // ========== 数据加载 ==========

    fn load_data_with_clauses(&self, page: usize, cx: &mut App) {
        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let table_name = self.config.table_name.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table = self.table.clone();
        let table_data_info = self.table_data_info.clone();
        let where_clause = self.filter_editor.read(cx).get_where_clause(cx);
        let order_by_clause = self.filter_editor.read(cx).get_order_by_clause(cx);
        let filter_editor = self.filter_editor.clone();
        let page_size = self.table_data_info.read(cx).page_size;
        let data_grid_entity = self.self_entity.clone();
        let vis_key = format!(
            "column_visibility:{}:{}:{}",
            self.config.connection_id, self.config.database_name, self.config.table_name
        );

        tracing::info!(
            "load_data_with_clauses: connection_id={}, database={}, table={}, WHERE={}, ORDER BY={}",
            connection_id,
            database_name,
            table_name,
            if where_clause.is_empty() {
                "(none)"
            } else {
                &where_clause
            },
            if order_by_clause.is_empty() {
                "(none)"
            } else {
                &order_by_clause
            }
        );

        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_loading(true);
            cx.notify();
        });

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut request = TableDataRequest::new(&database_name, &table_name)
                .with_page(page, page_size)
                .with_where_clause(where_clause.clone())
                .with_order_by_clause(order_by_clause.clone());

            if let Some(schema) = schema_name.clone() {
                request = request.with_schema(schema);
            }

            let result = global_state
                .query_table_data(cx, connection_id.clone(), request)
                .await;
            let columns_info = global_state
                .list_columns(cx, connection_id, database_name, schema_name, table_name)
                .await;

            match (result, columns_info) {
                (Err(err), _) => {
                    tracing::error!("load_data_with_clauses failed: {}", err);
                    cx.update(|cx| {
                        table_data_info.update(cx, |info, cx| {
                            info.error_message =
                                Some(t!("TableDataGrid.load_data_failed", error = err).to_string());
                            cx.notify();
                        });
                        table.update(cx, |state, cx| {
                            state.delegate_mut().set_loading(false);
                            state.delegate_mut().update_data(vec![], vec![], vec![], cx);
                            state.refresh(cx);
                        });
                    })
                }
                (Ok(response), columns_info_result) => {
                    let query_result = response.query_result;

                    let (columns, rows, rowids) =
                        if query_result.columns.first().map(|c| c.as_str()) == Some("__rowid__") {
                            let columns: Vec<Column> = query_result
                                .columns
                                .iter()
                                .skip(1)
                                .map(|col| Column::new(col.clone(), col.clone()))
                                .collect();
                            let mut rowids = Vec::new();
                            let rows: Vec<Vec<Option<String>>> = query_result
                                .rows
                                .iter()
                                .map(|row| {
                                    if let Some(first) = row.first() {
                                        rowids.push(first.clone().unwrap_or_default());
                                    }
                                    row.iter().skip(1).cloned().collect()
                                })
                                .collect();
                            (columns, rows, rowids)
                        } else {
                            let columns: Vec<Column> = query_result
                                .columns
                                .iter()
                                .map(|col| Column::new(col.clone(), col.clone()))
                                .collect();
                            let rows: Vec<Vec<Option<String>>> = query_result
                                .rows
                                .iter()
                                .map(|row| row.iter().cloned().collect())
                                .collect();
                            (columns, rows, Vec::new())
                        };

                    let column_meta = match columns_info_result {
                        Ok(cols) => cols,
                        Err(_) => query_result
                            .column_meta
                            .iter()
                            .filter(|meta| meta.name != "__rowid__")
                            .map(|meta| ColumnInfo {
                                name: meta.name.clone(),
                                data_type: meta.db_type.clone(),
                                is_nullable: meta.nullable,
                                is_primary_key: false,
                                default_value: None,
                                comment: None,
                                charset: None,
                                collation: None,
                            })
                            .collect(),
                    };

                    cx.update(|cx| {
                        table_data_info.update(cx, |info, cx| {
                            info.total_count = response.total_count;
                            info.current_sql = query_result.sql.clone();
                            info.duration = response.duration;
                            info.current_page = response.page;
                            info.columns = column_meta.clone();
                            info.index_infos = vec![];
                            info.error_message = None;
                            cx.notify();
                        });
                    });

                    cx.update(|cx| {
                        filter_editor.update(cx, |editor, cx| {
                            editor.set_schema(
                                TableSchema {
                                    columns: column_meta.clone(),
                                },
                                cx,
                            );
                        });

                        // 加载并应用列可见性配置
                        let storage = cx.try_global::<one_core::storage::GlobalStorageState>();
                        let hidden_columns: HashSet<SharedString> = if let Some(storage) = storage {
                            if let Some(kv_repo) = storage
                                .storage
                                .get::<one_core::storage::KeyValueRepository>()
                            {
                                if let Ok(Some(value)) = kv_repo.get_by_key(&vis_key) {
                                    if let Ok(hidden) = serde_json::from_str::<Vec<String>>(&value)
                                    {
                                        hidden.into_iter().map(SharedString::from).collect()
                                    } else {
                                        HashSet::new()
                                    }
                                } else {
                                    HashSet::new()
                                }
                            } else {
                                HashSet::new()
                            }
                        } else {
                            HashSet::new()
                        };

                        if let Some(dg) = data_grid_entity.as_ref().and_then(|e| e.upgrade()) {
                            dg.update(cx, |grid: &mut DataGrid, _cx| {
                                grid.hidden_columns = hidden_columns.clone();
                                grid.column_visibility_loaded = true;
                            });
                        }
                        table.update(cx, |state, cx| {
                            state.delegate_mut().set_loading(false);
                            state.delegate_mut().set_column_meta(column_meta);
                            state.delegate_mut().update_data(columns, rows, rowids, cx);
                            state.delegate_mut().apply_order_by_clause(&order_by_clause);
                            state.delegate_mut().update_visible_columns(&hidden_columns);
                            state.refresh(cx);
                        });
                    });
                }
            }
        })
        .detach();
    }

    fn handle_refresh(&self, cx: &mut App) {
        match self.config.usage {
            DataGridUsage::TableData => {
                let page = self.table_data_info.read(cx).current_page;
                self.load_data_with_clauses(page, cx);
            }
            DataGridUsage::SqlResult => {
                self.load_data_with_sql(self.config.sql.clone(), cx);
            }
        }
    }

    pub fn refresh_data(&self, cx: &mut App) {
        if self.table.read(cx).delegate().is_loading() {
            return;
        }
        self.handle_refresh(cx);
    }

    pub fn request_refresh(&self, window: &mut Window, cx: &mut App) {
        self.request_reload_action(ReloadAction::RefreshCurrent, window, cx);
    }

    pub fn open_large_text_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        match resolve_large_text_editor_route(
            current_db_view_settings(cx).large_text_editor_open_mode,
        ) {
            LargeTextEditorRoute::PagePreview => {
                cx.emit(DataGridEvent::ToggleLargeTextEditorRequested);
            }
            LargeTextEditorRoute::Dialog => {
                self.show_large_text_editor(window, cx);
            }
        }
    }

    pub fn set_large_text_editor_sidebar_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.is_large_text_editor_sidebar_open == open {
            return;
        }

        self.is_large_text_editor_sidebar_open = open;
        cx.notify();
    }

    pub fn open_export_view(&self, window: &mut Window, cx: &mut App) {
        if self.config.usage != DataGridUsage::TableData {
            window.push_notification(t!("TableDataGrid.export_not_supported").to_string(), cx);
            return;
        }

        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();

        let export_view = cx.new(|cx| {
            DataExportView::new(
                connection_id.clone(),
                database_name.clone(),
                schema_name.clone(),
                table_name.clone(),
                window,
                cx,
            )
        });

        let existing_columns = self.table_data_info.read(cx).columns.clone();
        if !existing_columns.is_empty() {
            export_view.update(cx, |view, cx| {
                view.update_column_list(existing_columns, cx);
            });
        } else {
            let global_state = cx.global::<GlobalDbState>().clone();
            let export_view_handle = export_view.clone();
            let connection_id_for_columns = self.config.connection_id.clone();
            let database_name_for_columns = self.config.database_name.clone();
            let schema_name_for_columns = self.config.schema_name.clone();
            let table_name_for_columns = self.config.table_name.clone();

            cx.spawn(async move |cx: &mut AsyncApp| {
                let columns_result = global_state
                    .list_columns(
                        cx,
                        connection_id_for_columns,
                        database_name_for_columns,
                        schema_name_for_columns,
                        table_name_for_columns,
                    )
                    .await;

                match columns_result {
                    Ok(columns) => {
                        cx.update(|cx| {
                            export_view_handle.update(cx, |view, cx| {
                                view.update_column_list(columns, cx);
                            });
                        });
                    }
                    Err(error) => {
                        error!("Failed to fetch export columns: {}", error);
                    }
                }
            })
            .detach();
        }

        open_popup_window(
            window,
            PopupWindowOptions::new(t!("TableDataGrid.export_table").to_string())
                .size(800.0, 600.0),
            move |_window, _cx| export_view.clone(),
            cx,
        );
    }

    fn export_result_set(
        &self,
        scope: ExportScope,
        format: ExportFormat,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.table.read(cx).delegate().is_loading() {
            window.push_notification(t!("TableDataGrid.export_loading").to_string(), cx);
            return;
        }

        let usage = self.config.usage.clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();
        let current_page = self.table_data_info.read(cx).current_page;
        let total_count = self.table_data_info.read(cx).total_count;
        let where_clause = self.filter_editor.read(cx).get_where_clause(cx);
        let order_by_clause = self.filter_editor.read(cx).get_order_by_clause(cx);
        let table = self.table.clone();
        let metadata = self.table.read(cx).delegate().get_table_metadata();
        let window_id = cx.active_window();
        let global_state = cx.global::<GlobalDbState>().clone();
        let prompt_future = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            multiple: false,
            directories: true,
            prompt: Some(t!("TableDataGrid.select_export_directory").into()),
        });

        cx.spawn(async move |cx: &mut AsyncApp| {
            let output_dir: PathBuf = match prompt_future.await {
                Ok(Ok(Some(paths))) => match paths.first() {
                    Some(path) => path.clone(),
                    None => return,
                },
                _ => return,
            };

            let export_payload = match scope {
                ExportScope::CurrentPage => Self::collect_visible_rows(&table, cx),
                ExportScope::All => match usage {
                    DataGridUsage::TableData => {
                        if total_count == 0 {
                            None
                        } else {
                            let mut request = TableDataRequest::new(&database_name, &table_name)
                                .with_page(1, total_count)
                                .with_where_clause(where_clause)
                                .with_order_by_clause(order_by_clause);

                            if let Some(schema) = schema_name.clone() {
                                request = request.with_schema(schema);
                            }

                            match global_state
                                .query_table_data(cx, connection_id.clone(), request)
                                .await
                            {
                                Ok(response) => {
                                    let query_result = response.query_result;
                                    let (columns, rows) =
                                        Self::normalize_query_result(query_result);
                                    Some((columns, rows))
                                }
                                Err(error) => {
                                    let _ = cx.update(|cx| {
                                        if let Some(window_id) = window_id {
                                            let _ = cx.update_window(
                                                window_id,
                                                |_entity, window, cx| {
                                                    window.push_notification(
                                                        t!(
                                                            "TableDataGrid.export_failed",
                                                            error = error
                                                        )
                                                        .to_string(),
                                                        cx,
                                                    );
                                                },
                                            );
                                        }
                                    });
                                    return;
                                }
                            }
                        }
                    }
                    DataGridUsage::SqlResult => Self::collect_visible_rows(&table, cx),
                },
            };

            let Some((columns, rows)) = export_payload else {
                let _ = cx.update(|cx| {
                    if let Some(window_id) = window_id {
                        let _ = cx.update_window(window_id, |_entity, window, cx| {
                            window.push_notification(
                                t!("TableDataGrid.no_data_to_export").to_string(),
                                cx,
                            );
                        });
                    }
                });
                return;
            };

            let bytes = match build_export_bytes(format, rows, columns.clone(), metadata.clone()) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => {
                    let _ = cx.update(|cx| {
                        if let Some(window_id) = window_id {
                            let _ = cx.update_window(window_id, |_entity, window, cx| {
                                window.push_notification(
                                    t!("TableDataGrid.no_data_to_export").to_string(),
                                    cx,
                                );
                            });
                        }
                    });
                    return;
                }
                Err(error) => {
                    let _ = cx.update(|cx| {
                        if let Some(window_id) = window_id {
                            let _ = cx.update_window(window_id, |_entity, window, cx| {
                                window.push_notification(
                                    t!("TableDataGrid.export_failed", error = error).to_string(),
                                    cx,
                                );
                            });
                        }
                    });
                    return;
                }
            };

            let base_name = if table_name.is_empty() {
                "result_set"
            } else {
                table_name.as_str()
            };
            let scope_suffix = match scope {
                ExportScope::All => "all".to_string(),
                ExportScope::CurrentPage => format!("page-{}", current_page),
            };
            let now = Local::now();
            let datetime_str = now.format("%Y-%m-%d_%H-%M-%S").to_string();
            let prefix = if database_name.is_empty() {
                base_name.to_string()
            } else {
                format!("{}_{}", database_name, base_name)
            };
            let filename = format!(
                "{}_{}_{}.{}",
                prefix,
                scope_suffix,
                datetime_str,
                format.extension()
            );
            let full_path = output_dir.join(filename);
            let full_path_for_write = full_path.clone();

            let write_result = cx
                .background_spawn(async move { std::fs::write(&full_path_for_write, bytes) })
                .await;

            let _ = cx.update(|cx| {
                if let Some(window_id) = window_id {
                    let _ = cx.update_window(window_id, |_entity, window, cx| match write_result {
                        Ok(()) => {
                            window.push_notification(
                                t!("TableDataGrid.export_complete", path = full_path.display())
                                    .to_string(),
                                cx,
                            );
                        }
                        Err(error) => {
                            window.push_notification(
                                t!("TableDataGrid.export_failed", error = error).to_string(),
                                cx,
                            );
                        }
                    });
                }
            });
        })
        .detach();
    }

    fn collect_visible_rows(
        table: &Entity<EditTableState<EditorTableDelegate>>,
        cx: &AsyncApp,
    ) -> Option<(Vec<SharedString>, Vec<Vec<Option<String>>>)> {
        table.read_with(cx, |table_state, _cx| {
            let delegate = table_state.delegate();
            let row_count = delegate.filtered_row_count();
            if row_count == 0 {
                return None;
            }

            let mut row_indices = Vec::with_capacity(row_count);
            for display_row in 0..row_count {
                let Some(actual_row) = delegate.resolve_display_row(display_row) else {
                    continue;
                };
                if delegate.is_deleted_row(actual_row) {
                    continue;
                }
                row_indices.push(actual_row);
            }

            let rows = delegate.get_rows_data(&row_indices);
            if rows.is_empty() {
                return None;
            }

            let columns = delegate
                .columns
                .iter()
                .map(|column| column.name.clone())
                .collect();

            Some((columns, rows))
        })
    }

    fn normalize_query_result(
        query_result: QueryResult,
    ) -> (Vec<SharedString>, Vec<Vec<Option<String>>>) {
        if query_result.columns.first().map(|name| name.as_str()) == Some("__rowid__") {
            let columns = query_result
                .columns
                .into_iter()
                .skip(1)
                .map(SharedString::from)
                .collect();
            let rows = query_result
                .rows
                .into_iter()
                .map(|row| row.into_iter().skip(1).collect())
                .collect();
            (columns, rows)
        } else {
            let columns = query_result
                .columns
                .into_iter()
                .map(SharedString::from)
                .collect();
            (columns, query_result.rows)
        }
    }

    fn load_data_with_sql(&self, sql: String, cx: &mut App) {
        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table = self.table.clone();
        let table_name = self.config.table_name.clone();
        let editable = self.config.editable;

        cx.spawn(async move |cx: &mut AsyncApp| {
            let result: Result<(SqlResult, Option<Vec<ColumnInfo>>), anyhow::Error> = async {
                let sql_result = global_state
                    .execute_single(
                        cx,
                        connection_id.clone(),
                        sql.clone(),
                        Some(database_name.clone()),
                        None,
                    )
                    .await?;
                if editable {
                    let column_meta = global_state
                        .list_columns(
                            cx,
                            connection_id.clone(),
                            database_name.clone(),
                            schema_name.clone(),
                            table_name.clone(),
                        )
                        .await?;
                    return Ok((sql_result, Some(column_meta)));
                }
                Ok((sql_result, None))
            }
            .await;

            match result {
                Err(err) => cx.update(|cx| {
                    notification(
                        cx,
                        t!("TableDataGrid.execute_sql_failed", error = err.to_string()).to_string(),
                    );
                }),
                Ok(results) => {
                    let (result, column_meta) = results;
                    if let SqlResult::Query(query_result) = result {
                        let columns: Vec<Column> = query_result
                            .columns
                            .iter()
                            .map(|col| Column::new(col.clone(), col.clone()))
                            .collect();
                        let rows: Vec<Vec<Option<String>>> = query_result
                            .rows
                            .iter()
                            .map(|row| row.iter().cloned().collect())
                            .collect();
                        cx.update(|cx| {
                            table.update(cx, |state, cx| {
                                if let Some(columns) = column_meta {
                                    state.delegate_mut().set_column_meta(columns);
                                }
                                state.delegate_mut().update_data(columns, rows, vec![], cx);
                                state.refresh(cx);
                            });
                        })
                    }
                }
            }
        })
        .detach();
    }

    fn handle_prev_page(&self, window: &mut Window, cx: &mut App) {
        let page = self.table_data_info.read(cx).current_page;
        if page > 1 {
            self.request_reload_action(ReloadAction::LoadPage(page - 1), window, cx);
        }
    }

    fn handle_next_page(&self, window: &mut Window, cx: &mut App) {
        let info = self.table_data_info.read(cx);
        let page = info.current_page;
        let total = info.total_count;
        let page_size = info.page_size;

        if page_size == 0 {
            return;
        }
        let total_pages = total.div_ceil(page_size);
        if page < total_pages {
            self.request_reload_action(ReloadAction::LoadPage(page + 1), window, cx);
        }
    }

    fn handle_page_size_change(&self, new_size: usize, window: &mut Window, cx: &mut App) {
        self.request_reload_action(ReloadAction::ChangePageSize(new_size), window, cx);
    }

    fn handle_page_change_500(&mut self, _: &Page500, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_page_size_change(500, window, cx)
    }

    fn handle_page_change_100(&mut self, _: &Page100, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_page_size_change(100, window, cx)
    }

    fn handle_page_change_200(&mut self, _: &Page200, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_page_size_change(200, window, cx)
    }

    fn handle_page_change_300(&mut self, _: &Page300, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_page_size_change(300, window, cx)
    }

    fn handle_page_change_1000(
        &mut self,
        _: &Page1000,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_page_size_change(1000, window, cx)
    }

    fn handle_add_row(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| state.add_row(window, cx));
    }

    fn handle_delete_row(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            // 优先使用多选区对应的行；没有多选时回退到旧的单选兼容状态。
            let row_indices = collect_delete_row_indices(
                state
                    .selection()
                    .all_cells()
                    .into_iter()
                    .map(|(row_ix, _)| row_ix),
                state
                    .selected_row()
                    .or_else(|| state.selected_cell().map(|(row_ix, _)| row_ix)),
            );

            for row_ix in row_indices {
                state.delete_row(row_ix, window, cx);
            }
        });
    }

    fn handle_revert_changes(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.revert_changes(cx);
    }

    fn handle_step_undo(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_undo(cx);
    }

    fn handle_sql_preview(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            state.commit_cell_edit(window, cx);
        });
        self.show_sql_preview(window, cx);
    }

    fn handle_commit_changes(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table.update(cx, |state, cx| {
            state.commit_cell_edit(window, cx);
        });
        self.handle_save_changes(event, window, cx);
    }

    fn handle_large_text_editor(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_large_text_editor(window, cx);
    }

    fn handle_toolbar_refresh(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_refresh(window, cx);
    }

    fn handle_prev_page_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_prev_page(window, cx);
    }

    fn handle_next_page_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_next_page(window, cx);
    }

    // ========== 大文本编辑器 ==========

    fn show_large_text_editor(&self, window: &mut Window, cx: &mut App) {
        let Some(target) = self.selected_large_text_target(cx) else {
            window.push_notification(t!("TableData.select_cell").to_string(), cx);
            return;
        };

        self.show_text_editor_dialog(
            target.value,
            &build_large_text_editor_title(&target.column_name, target.display_row_ix),
            target.display_row_ix,
            target.col_ix + 1,
            target.editable,
            window,
            cx,
        );
    }

    pub fn selected_large_text_target(&self, cx: &App) -> Option<LargeTextCellTarget> {
        let table = self.table.read(cx);
        let (display_row_ix, selected_col_ix) = table.selected_cell()?;
        let col_ix = selected_col_ix.checked_sub(1)?;
        let delegate = table.delegate();
        let actual_row_ix = delegate.resolve_display_row(display_row_ix)?;
        let value = delegate
            .rows
            .get(actual_row_ix)
            .and_then(|row| row.get(col_ix))
            .cloned()
            .flatten()
            .unwrap_or_default();
        let column_name = delegate
            .columns
            .get(col_ix)
            .map(|col| col.name.to_string())
            .unwrap_or_else(|| {
                t!("TableDataGrid.column_label", index = selected_col_ix).to_string()
            });

        Some(LargeTextCellTarget {
            display_row_ix,
            actual_row_ix,
            col_ix,
            column_name,
            value,
            editable: self.config.editable,
        })
    }

    pub fn apply_large_text_target_value(
        &self,
        target: &LargeTextCellTarget,
        value: String,
        cx: &mut App,
    ) -> bool {
        self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            let changed = delegate.record_cell_change(target.actual_row_ix, target.col_ix, value);

            if changed {
                state.refresh(cx);
            }

            changed
        })
    }

    fn show_text_editor_dialog(
        &self,
        initial_text: String,
        title: &str,
        row_ix: usize,
        col_ix: usize,
        editable: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let dialog_text_editor =
            create_large_text_editor_with_content(Some(initial_text.clone()), window, cx);
        let data_grid = self.clone();
        let title = title.to_string();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor = dialog_text_editor.clone();
            let data_grid = data_grid.clone();
            let original_text = initial_text.clone();

            let mut d = dialog
                .title(SharedString::from(title.clone()))
                .w(px(800.0))
                .h(px(600.0))
                .child(v_flex().w_full().h_full().child(editor.clone()))
                .close_button(true)
                .overlay(false)
                .content_center();

            if editable {
                d = d
                    .footer(|ok, cancel, window, cx| vec![ok(window, cx), cancel(window, cx)])
                    .on_ok(move |_, window, cx| {
                        if !editor.read(cx).has_pending_writeback() {
                            return true;
                        }
                        let content = editor.read(cx).get_writeback_text(cx);
                        return match content {
                            Ok(val) => {
                                if large_text_values_equivalent(&original_text, &val) {
                                    editor.update(cx, |editor, _| {
                                        editor.mark_writeback_clean();
                                    });
                                    return true;
                                }
                                editor.update(cx, |editor, _| {
                                    editor.mark_writeback_clean();
                                });
                                data_grid.table.update(cx, |state, cx| {
                                    let delegate = state.delegate_mut();
                                    let Some(actual_row_ix) = delegate.resolve_display_row(row_ix)
                                    else {
                                        return false;
                                    };

                                    let col_index = col_ix.saturating_sub(1);
                                    let changed =
                                        delegate.record_cell_change(actual_row_ix, col_index, val);

                                    if changed {
                                        state.refresh(cx);
                                    }

                                    changed
                                });
                                true
                            }
                            Err(err) => {
                                window.push_notification(
                                    t!("TableDataGrid.error_message", error = err).to_string(),
                                    cx,
                                );
                                false
                            }
                        };
                    });
            } else {
                d = d.footer(|_ok, cancel, window, cx| vec![cancel(window, cx)]);
            }

            d
        });
    }

    // ========== 数据变更 ==========

    pub fn get_changes(&self, cx: &App) -> Vec<RowChange> {
        self.table.read(cx).delegate().get_changes()
    }

    pub fn column_names(&self, cx: &App) -> Vec<String> {
        self.table.read(cx).delegate().column_names()
    }

    pub fn clear_changes(&self, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().clear_changes();
            cx.notify();
        });
    }

    pub fn accept_saved_changes(&self, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().accept_current_state_as_saved();
            state.refresh(cx);
            cx.notify();
        });
    }

    pub fn revert_changes(&self, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().revert_all_changes();
            state.refresh(cx);
            cx.notify();
        });
    }

    /// Perform a single step undo operation
    pub fn step_undo(&self, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            if delegate.undo() {
                state.refresh(cx);
                cx.notify();
            }
        });
    }

    /// Check if step undo is available
    pub fn can_step_undo(&self, cx: &App) -> bool {
        self.table.read(cx).delegate().can_undo()
    }

    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        let table = self.table.read(cx);
        table.editing_cell().is_some() || !table.delegate().get_changes().is_empty()
    }

    pub fn get_page_info(&self, cx: &App) -> (usize, usize) {
        let info = self.table_data_info.read(cx);
        (info.current_page, info.page_size)
    }

    pub fn pending_change_level(&self, cx: &App) -> Option<one_core::PendingChangeLevel> {
        self.table.read(cx).delegate().pending_change_level()
    }

    // ========== 复制为 SQL 语句 ==========

    /// 复制选中行为 INSERT 语句
    pub fn copy_as_insert(&self, row_indices: &[usize], cx: &App) -> String {
        use db::CopySqlRequest;

        let table = self.table.read(cx);
        let delegate = table.delegate();

        let rows_data = delegate.get_rows_data(row_indices);
        if rows_data.is_empty() {
            return String::new();
        }

        let column_names = delegate.column_names();
        let columns_meta = delegate.column_meta().to_vec();

        let global_state = cx.global::<GlobalDbState>().clone();
        match global_state
            .db_manager
            .get_plugin(&self.config.database_type)
        {
            Ok(plugin) => {
                let mut request = CopySqlRequest::new(&self.config.table_name, columns_meta)
                    .with_rows(rows_data)
                    .with_column_names(column_names);
                if let Some(schema) = &self.config.schema_name {
                    request = request.with_schema(schema);
                }
                plugin.generate_copy_insert_sql(&request)
            }
            Err(_) => String::new(),
        }
    }

    /// 复制选中行为 INSERT 语句（带字段注释）
    pub fn copy_as_insert_with_comments(&self, row_indices: &[usize], cx: &App) -> String {
        use db::CopySqlRequest;

        let table = self.table.read(cx);
        let delegate = table.delegate();

        let rows_data = delegate.get_rows_data(row_indices);
        if rows_data.is_empty() {
            return String::new();
        }

        let column_names = delegate.column_names();
        let columns_meta = delegate.column_meta().to_vec();

        let global_state = cx.global::<GlobalDbState>().clone();
        match global_state
            .db_manager
            .get_plugin(&self.config.database_type)
        {
            Ok(plugin) => {
                let mut request = CopySqlRequest::new(&self.config.table_name, columns_meta)
                    .with_rows(rows_data)
                    .with_column_names(column_names);
                if let Some(schema) = &self.config.schema_name {
                    request = request.with_schema(schema);
                }
                plugin.generate_copy_insert_with_comments_sql(&request)
            }
            Err(_) => String::new(),
        }
    }

    /// 复制选中行为 UPDATE 语句
    pub fn copy_as_update(&self, row_indices: &[usize], cx: &App) -> String {
        use db::CopySqlRequest;

        let table = self.table.read(cx);
        let delegate = table.delegate();

        let rows_data = delegate.get_rows_data(row_indices);
        let original_rows = delegate.get_original_rows_data(row_indices);
        if rows_data.is_empty() {
            return String::new();
        }

        let column_names = delegate.column_names();
        let columns_meta = delegate.column_meta().to_vec();

        let global_state = cx.global::<GlobalDbState>().clone();
        match global_state
            .db_manager
            .get_plugin(&self.config.database_type)
        {
            Ok(plugin) => {
                let mut request = CopySqlRequest::new(&self.config.table_name, columns_meta)
                    .with_rows(rows_data)
                    .with_original_rows(original_rows)
                    .with_column_names(column_names);
                if let Some(schema) = &self.config.schema_name {
                    request = request.with_schema(schema);
                }
                plugin.generate_copy_update_sql(&request)
            }
            Err(_) => String::new(),
        }
    }

    /// 复制选中行为 DELETE 语句
    pub fn copy_as_delete(&self, row_indices: &[usize], cx: &App) -> String {
        use db::CopySqlRequest;

        let table = self.table.read(cx);
        let delegate = table.delegate();

        let rows_data = delegate.get_rows_data(row_indices);
        if rows_data.is_empty() {
            return String::new();
        }

        let column_names = delegate.column_names();
        let columns_meta = delegate.column_meta().to_vec();

        let global_state = cx.global::<GlobalDbState>().clone();
        match global_state
            .db_manager
            .get_plugin(&self.config.database_type)
        {
            Ok(plugin) => {
                let mut request = CopySqlRequest::new(&self.config.table_name, columns_meta)
                    .with_rows(rows_data)
                    .with_column_names(column_names);
                if let Some(schema) = &self.config.schema_name {
                    request = request.with_schema(schema);
                }
                plugin.generate_copy_delete_sql(&request)
            }
            Err(_) => String::new(),
        }
    }

    /// 获取当前选中的行索引列表
    pub fn selected_row_indices(&self, cx: &App) -> Vec<usize> {
        let table = self.table.read(cx);
        let selection = table.selection();

        if selection.is_empty() {
            // 如果没有多选，检查单选的行
            if let Some(row_ix) = table.selected_row() {
                return vec![row_ix];
            }
            return vec![];
        }

        // 从选区获取所有行
        let mut rows: Vec<usize> = selection.all_cells().iter().map(|(row, _)| *row).collect();
        rows.sort();
        rows.dedup();
        rows
    }

    /// 复制当前选中行为指定类型的 SQL
    pub fn copy_selection_as_sql(&self, sql_type: db::CopyAsSqlType, cx: &App) -> String {
        use db::CopyAsSqlType;

        let row_indices = self.selected_row_indices(cx);
        if row_indices.is_empty() {
            return String::new();
        }

        match sql_type {
            CopyAsSqlType::Insert => self.copy_as_insert(&row_indices, cx),
            CopyAsSqlType::InsertWithComments => {
                self.copy_as_insert_with_comments(&row_indices, cx)
            }
            CopyAsSqlType::Update => self.copy_as_update(&row_indices, cx),
            CopyAsSqlType::Delete => self.copy_as_delete(&row_indices, cx),
        }
    }

    pub fn convert_row_changes(
        changes: Vec<RowChange>,
        column_names: &[String],
    ) -> Vec<TableRowChange> {
        changes
            .into_iter()
            .filter_map(|change| match change {
                RowChange::Added { data } => Some(TableRowChange::Added {
                    data: data
                        .into_iter()
                        .map(|opt| opt.unwrap_or_default())
                        .collect(),
                }),
                RowChange::Updated {
                    original_data,
                    changes,
                    rowid,
                } => {
                    let converted: Vec<TableCellChange> = changes
                        .into_iter()
                        .map(|c| TableCellChange {
                            column_index: c.col_ix,
                            column_name: if c.col_name.is_empty() {
                                column_names.get(c.col_ix).cloned().unwrap_or_default()
                            } else {
                                c.col_name
                            },
                            old_value: c.old_value.unwrap_or_default(),
                            new_value: c.new_value.unwrap_or_default(),
                        })
                        .collect();

                    if converted.is_empty() {
                        None
                    } else {
                        Some(TableRowChange::Updated {
                            original_data: original_data
                                .into_iter()
                                .map(|opt| opt.unwrap_or_default())
                                .collect(),
                            changes: converted,
                            rowid,
                        })
                    }
                }
                RowChange::Deleted {
                    original_data,
                    rowid,
                } => Some(TableRowChange::Deleted {
                    original_data: original_data
                        .into_iter()
                        .map(|opt| opt.unwrap_or_default())
                        .collect(),
                    rowid,
                }),
            })
            .collect()
    }

    pub fn create_save_request(
        &self,
        columns: Vec<ColumnInfo>,
        index_infos: Vec<IndexInfo>,
        cx: &App,
    ) -> Option<TableSaveRequest> {
        let changes = self.get_changes(cx);
        if changes.is_empty() {
            return None;
        }

        let column_names = self.column_names(cx);
        let table_changes = Self::convert_row_changes(changes, &column_names);

        if table_changes.is_empty() {
            return None;
        }

        let driver_id = if self.config.database_type == DatabaseType::External {
            cx.try_global::<GlobalDbState>()
                .and_then(|state| state.get_config(&self.config.connection_id))
                .and_then(|config| config.get_param(EXTERNAL_DRIVER_ID_PARAM).cloned())
        } else {
            None
        };

        Some(TableSaveRequest {
            database: self.config.database_name.clone(),
            schema: self.config.schema_name.clone(),
            table: self.config.table_name.clone(),
            columns,
            index_infos,
            changes: table_changes,
            driver_id,
        })
    }

    pub fn save_changes(&self, window: &mut Window, cx: &mut App) {
        self.table.update(cx, |state, cx| {
            state.commit_cell_edit(window, cx);
        });
        self.handle_save_changes(&gpui::ClickEvent::default(), window, cx);
    }

    pub fn save_and_close(
        &self,
        tab_container: Entity<TabContainer>,
        tab_id: String,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let changes = self.get_changes(cx);
        if changes.is_empty() {
            if let Some(window_id) = cx.active_window() {
                let _ = cx.update_window(window_id, |_, _window, cx| {
                    tab_container.update(cx, |container, cx| {
                        container.force_close_tab_by_id(&tab_id, cx);
                    });
                });
            }
            return;
        }

        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();
        let database_type = self.config.database_type;
        let this = self.clone();
        let delegate = self.table.read(cx).delegate();
        let columns = delegate.column_meta.clone();
        let has_primary_key = columns.iter().any(|c| c.is_primary_key);
        let has_rowids = delegate.has_rowids();
        let need_index_infos = !has_primary_key && !has_rowids;

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut index_infos = vec![];
            if need_index_infos {
                let index_infos_result = global_state
                    .list_indexes(
                        cx,
                        connection_id.clone(),
                        database_name.clone(),
                        schema_name.clone(),
                        table_name.clone(),
                    )
                    .await;
                index_infos = match index_infos_result {
                    Ok(infos) => infos,
                    Err(err) => {
                        cx.update(|cx| {
                            notification(
                                cx,
                                t!(
                                    "TableDataGrid.get_table_keys_failed",
                                    error = err.to_string()
                                )
                                .to_string(),
                            );
                        });
                        return;
                    }
                };
            }

            let save_result = cx.update(|cx| {
                let Some(save_request) = this.create_save_request(columns, index_infos, cx) else {
                    return Err(t!("TableDataGrid.no_changes").to_string());
                };
                let change_count = save_request.changes.len();

                let global_state = cx.global::<GlobalDbState>().clone();
                match global_state.db_manager.get_plugin(&database_type) {
                    Ok(plugin) => {
                        let sql = plugin.generate_table_changes_sql(&save_request);
                        let trimmed = sql.trim();
                        if trimmed.is_empty()
                            || trimmed == t!("TableDataGrid.no_changes_sql_marker")
                        {
                            Err(t!("TableDataGrid.no_changes").to_string())
                        } else {
                            Ok((sql, change_count))
                        }
                    }
                    Err(_) => Err(t!("TableDataGrid.plugin_unavailable").to_string()),
                }
            });

            let (sql_content, change_count) = match save_result {
                Ok((sql, count)) => (sql, count),
                Err(msg) => {
                    cx.update(|cx| notification(cx, msg));
                    return;
                }
            };

            let exec_options = ExecOptions {
                stop_on_error: true,
                transactional: true,
                max_rows: None,
                streaming: false,
            };

            let result = global_state
                .execute_script(
                    cx,
                    connection_id.clone(),
                    sql_content.clone(),
                    Some(database_name.clone()),
                    None,
                    Some(exec_options),
                )
                .await;

            cx.update(|cx| match result {
                Ok(results) => {
                    if let Some(err_msg) = results.iter().find_map(|res| match res {
                        SqlResult::Error(err) => Some(err.message.clone()),
                        _ => None,
                    }) {
                        notification(
                            cx,
                            t!("TableDataGrid.save_changes_failed", error = err_msg).to_string(),
                        );
                    } else {
                        this.accept_saved_changes(cx);
                        this.refresh_data(cx);
                        notification(
                            cx,
                            t!("TableDataGrid.save_changes_success", count = change_count)
                                .to_string(),
                        );
                        if let Some(window_id) = cx.active_window() {
                            let _ = cx.update_window(window_id, |_, _window, cx| {
                                tab_container.update(cx, |container, cx| {
                                    container.force_close_tab_by_id(&tab_id, cx);
                                });
                            });
                        }
                    }
                }
                Err(e) => {
                    notification(
                        cx,
                        t!("TableDataGrid.save_changes_failed", error = e.to_string()).to_string(),
                    );
                }
            });
        })
        .detach();
    }

    fn handle_save_changes(&self, _: &ClickEvent, _window: &mut Window, cx: &mut App) {
        let changes = self.get_changes(cx);
        if changes.is_empty() {
            return;
        }

        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();
        let database_type = self.config.database_type;
        let this = self.clone();
        let delegate = self.table.read(cx).delegate();
        let columns = delegate.column_meta.clone();
        let has_primary_key = columns.iter().any(|c| c.is_primary_key);
        let has_rowids = delegate.has_rowids();
        let need_index_infos = !has_primary_key && !has_rowids;

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut index_infos = vec![];
            if need_index_infos {
                let index_infos_result = global_state
                    .list_indexes(
                        cx,
                        connection_id.clone(),
                        database_name.clone(),
                        schema_name.clone(),
                        table_name.clone(),
                    )
                    .await;
                index_infos = match index_infos_result {
                    Ok(infos) => infos,
                    Err(err) => {
                        cx.update(|cx| {
                            notification(
                                cx,
                                t!(
                                    "TableDataGrid.get_table_keys_failed",
                                    error = err.to_string()
                                )
                                .to_string(),
                            );
                        });
                        return;
                    }
                };
            }

            let save_result = cx.update(|cx| {
                let Some(save_request) = this.create_save_request(columns, index_infos, cx) else {
                    return Err(t!("TableDataGrid.no_changes").to_string());
                };
                let change_count = save_request.changes.len();

                let global_state = cx.global::<GlobalDbState>().clone();
                match global_state.db_manager.get_plugin(&database_type) {
                    Ok(plugin) => {
                        let sql = plugin.generate_table_changes_sql(&save_request);
                        let trimmed = sql.trim();
                        if trimmed.is_empty()
                            || trimmed == t!("TableDataGrid.no_changes_sql_marker")
                        {
                            Err(t!("TableDataGrid.no_changes").to_string())
                        } else {
                            Ok((sql, change_count))
                        }
                    }
                    Err(_) => Err(t!("TableDataGrid.plugin_unavailable").to_string()),
                }
            });

            let (sql_content, change_count) = match save_result {
                Ok((sql, count)) => (sql, count),
                Err(msg) => {
                    cx.update(|cx| notification(cx, msg));
                    return;
                }
            };

            let exec_options = ExecOptions {
                stop_on_error: true,
                transactional: true,
                max_rows: None,
                streaming: false,
            };

            let result = global_state
                .execute_script(
                    cx,
                    connection_id.clone(),
                    sql_content.clone(),
                    Some(database_name.clone()),
                    None,
                    Some(exec_options),
                )
                .await;

            cx.update(|cx| match result {
                Ok(results) => {
                    if let Some(err_msg) = results.iter().find_map(|res| match res {
                        SqlResult::Error(err) => Some(err.message.clone()),
                        _ => None,
                    }) {
                        notification(
                            cx,
                            t!("TableDataGrid.save_changes_failed", error = err_msg).to_string(),
                        );
                    } else {
                        this.accept_saved_changes(cx);
                        this.refresh_data(cx);
                        notification(
                            cx,
                            t!("TableDataGrid.save_changes_success", count = change_count)
                                .to_string(),
                        );
                    }
                }
                Err(e) => {
                    notification(
                        cx,
                        t!("TableDataGrid.save_changes_failed", error = e.to_string()).to_string(),
                    );
                }
            });
        })
        .detach();
    }

    pub fn show_sql_preview(&self, window: &mut Window, cx: &mut App) {
        let changes = self.get_changes(cx);
        if changes.is_empty() {
            window.push_notification(t!("TableDataGrid.no_changes").to_string(), cx);
            return;
        }

        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let schema_name = self.config.schema_name.clone();
        let table_name = self.config.table_name.clone();
        let this = self.clone();
        let delegate = self.table.read(cx).delegate();
        let columns = delegate.column_meta.clone();
        let has_primary_key = columns.iter().any(|c| c.is_primary_key);
        let has_rowids = delegate.has_rowids();
        let need_index_infos = !has_primary_key && !has_rowids;

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut index_infos = vec![];
            if need_index_infos {
                let index_infos_result = global_state
                    .list_indexes(
                        cx,
                        connection_id.clone(),
                        database_name.clone(),
                        schema_name.clone(),
                        table_name.clone(),
                    )
                    .await;
                index_infos = match index_infos_result {
                    Ok(index_infos) => index_infos,
                    Err(err) => {
                        cx.update(|cx| {
                            notification(
                                cx,
                                t!(
                                    "TableDataGrid.get_table_keys_failed",
                                    error = err.to_string()
                                )
                                .to_string(),
                            );
                        });
                        return;
                    }
                };
            }

            cx.update(|cx| {
                if let Some(window_id) = cx.active_window() {
                    let _ = cx.update_window(window_id, |_entity, window, cx| {
                        let Some(save_request) =
                            this.create_save_request(columns.clone(), index_infos.clone(), cx)
                        else {
                            window
                                .push_notification(t!("TableDataGrid.no_changes").to_string(), cx);
                            return;
                        };

                        let sql_content = match this.build_changes_sql(&save_request, cx) {
                            Ok(sql) => sql,
                            Err(message) => {
                                window.push_notification(message, cx);
                                return;
                            }
                        };

                        this.show_sql_editor_dialog(
                            sql_content,
                            t!("TableDataGrid.change_sql_preview").as_ref(),
                            window,
                            cx,
                        );
                    });
                }
            });
        })
        .detach();
    }

    pub fn show_sql_editor_dialog(
        &self,
        initial_sql: String,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let sql_editor = cx.new(|cx| SqlEditor::new(window, cx));
        sql_editor.update(cx, |editor, cx| {
            editor.set_value(initial_sql, window, cx);
        });

        let global_state = cx.global::<GlobalDbState>().clone();
        let connection_id = self.config.connection_id.clone();
        let database_name = self.config.database_name.clone();
        let this = self.clone();
        let title = title.to_string();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let editor = sql_editor.clone();
            let execute_state = global_state.clone();
            let execute_connection = connection_id.clone();
            let execute_database = database_name.clone();
            let data_grid = this.clone();

            dialog
                .title(SharedString::from(title.clone()))
                .w(px(800.0))
                .h(px(600.0))
                .child(v_flex().w_full().h_full().child(editor.clone()))
                .close_button(true)
                .overlay(false)
                .content_center()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("TableDataGrid.execute_sql").to_string()),
                )
                .footer(|ok, cancel, window, cx| vec![ok(window, cx), cancel(window, cx)])
                .on_ok(move |_, window, cx| {
                    let sql_text = editor.read(cx).get_text(cx);
                    if sql_text.trim().is_empty() {
                        window.push_notification(t!("TableDataGrid.sql_empty").to_string(), cx);
                        return false;
                    }
                    data_grid.execute_sql_and_refresh(
                        sql_text,
                        execute_state.clone(),
                        execute_connection.clone(),
                        execute_database.clone(),
                        window,
                        cx,
                    );
                    false
                })
        });
    }

    async fn execute_sql_and_refresh_async(
        sql: String,
        global_state: GlobalDbState,
        connection_id: String,
        database_name: String,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let exec_options = ExecOptions {
            stop_on_error: true,
            transactional: true,
            max_rows: None,
            streaming: false,
        };

        let result = global_state
            .execute_script(
                cx,
                connection_id.clone(),
                sql.clone(),
                Some(database_name.clone()),
                None,
                Some(exec_options),
            )
            .await;

        match result {
            Ok(results) => {
                if let Some(err_msg) = results.iter().find_map(|res| match res {
                    SqlResult::Error(err) => Some(err.message.clone()),
                    _ => None,
                }) {
                    Err(t!("TableDataGrid.execute_failed", error = err_msg).to_string())
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(t!("TableDataGrid.execute_failed", error = e).to_string()),
        }
    }

    fn execute_sql_and_refresh(
        &self,
        sql: String,
        global_state: GlobalDbState,
        connection_id: String,
        database_name: String,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let data_grid = self.clone();

        cx.spawn(async move |cx: &mut AsyncApp| {
            match Self::execute_sql_and_refresh_async(
                sql,
                global_state,
                connection_id,
                database_name,
                cx,
            )
            .await
            {
                Ok(_) => {
                    cx.update(|cx| {
                        if let Some(window_id) = cx.active_window() {
                            let _ = cx.update_window(window_id, |_entity, window, cx| {
                                data_grid.accept_saved_changes(cx);
                                data_grid.refresh_data(cx);
                                window.close_dialog(cx);
                                window.push_notification(
                                    t!("TableDataGrid.execute_success").to_string(),
                                    cx,
                                );
                            });
                        }
                    });
                }
                Err(error_msg) => {
                    cx.update(|cx| {
                        if let Some(window_id) = cx.active_window() {
                            let _ = cx.update_window(window_id, |_entity, window, cx| {
                                window.push_notification(error_msg, cx);
                            });
                        }
                    });
                }
            }
        })
        .detach();
    }

    fn build_changes_sql(&self, request: &TableSaveRequest, cx: &App) -> Result<String, String> {
        let global_state = cx.global::<GlobalDbState>().clone();
        match global_state
            .db_manager
            .get_plugin(&self.config.database_type)
        {
            Ok(plugin) => {
                let sql = plugin.generate_table_changes_sql(request);
                let trimmed = sql.trim();
                if trimmed.is_empty() || trimmed == t!("TableDataGrid.no_changes_sql_marker") {
                    Err(t!("TableDataGrid.no_changes").to_string())
                } else {
                    Ok(plugin.format_sql(&sql))
                }
            }
            Err(_) => Err(t!("TableDataGrid.plugin_unavailable").to_string()),
        }
    }

    // ========== 渲染辅助方法 ==========

    pub fn render_toolbar(&self, _window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let editable = self.config.editable;
        let loading = self.table.read(cx).delegate().is_loading();
        let has_changes = self.has_unsaved_changes(cx);
        let data_grid = cx.entity().clone();
        let large_text_button_selected = current_db_view_settings(cx).large_text_editor_open_mode
            == LargeTextEditorOpenMode::SidebarPreview
            && self.is_large_text_editor_sidebar_open;

        h_flex()
            .gap_1()
            .items_center()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                Button::new("refresh-data")
                    .with_size(Size::Medium)
                    .icon(IconName::Refresh)
                    .tooltip(t!("TableDataGrid.refresh").to_string())
                    .border_color(cx.theme().border.opacity(0.5))
                    .disabled(loading)
                    .on_click(cx.listener(Self::handle_toolbar_refresh)),
            )
            .when(self.config.usage == DataGridUsage::TableData, |this| {
                this.child(
                    div().w(px(220.)).child(
                        Input::new(&self.search_input)
                            .prefix(
                                Icon::new(IconName::Search).text_color(cx.theme().muted_foreground),
                            )
                            .cleanable(true)
                            .small()
                            .w_full(),
                    ),
                )
            })
            .when(editable, |this| {
                this.child(
                    Button::new("add-row")
                        .with_size(Size::Medium)
                        .icon(IconName::Plus)
                        .tooltip(t!("TableDataGrid.add_row").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading)
                        .on_click(cx.listener(Self::handle_add_row)),
                )
            })
            .when(editable, |this| {
                this.child(
                    Button::new("delete-row")
                        .with_size(Size::Medium)
                        .icon(IconName::Minus)
                        .tooltip(t!("TableDataGrid.delete_row").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading)
                        .on_click(cx.listener(Self::handle_delete_row)),
                )
            })
            .when(editable, |this| {
                this.child(
                    Button::new("undo-changes")
                        .with_size(Size::Medium)
                        .icon(IconName::Undo)
                        .tooltip(t!("TableDataGrid.undo").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading || !has_changes)
                        .on_click(cx.listener(Self::handle_revert_changes)),
                )
            })
            .when(editable, |this| {
                let can_undo = self.can_step_undo(cx);
                this.child(
                    Button::new("step-undo")
                        .with_size(Size::Medium)
                        .icon(IconName::ArrowLeft)
                        .tooltip(t!("TableDataGrid.step_undo").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading || !can_undo)
                        .on_click(cx.listener(Self::handle_step_undo)),
                )
            })
            .when(editable, |this| {
                this.child(
                    Button::new("sql-preview")
                        .with_size(Size::Medium)
                        .icon(IconName::Eye)
                        .tooltip(t!("TableDataGrid.sql_preview").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading)
                        .on_click(cx.listener(Self::handle_sql_preview)),
                )
            })
            .when(editable, |this| {
                this.child(
                    Button::new("commit-changes")
                        .with_size(Size::Medium)
                        .icon(IconName::ArrowUp)
                        .tooltip(t!("TableDataGrid.commit_changes").to_string())
                        .border_color(cx.theme().border.opacity(0.5))
                        .disabled(loading || !has_changes)
                        .on_click(cx.listener(Self::handle_commit_changes)),
                )
            })
            .child(div().flex_1())
            .child({
                let data_grid_entity = cx.entity();
                Button::new("column-visibility")
                    .with_size(Size::Medium)
                    .icon(IconName::ListCheck)
                    .tooltip(t!("TableDataGrid.column_visibility").to_string())
                    .border_color(cx.theme().border.opacity(0.5))
                    .disabled(loading)
                    .dropdown_menu(move |menu, _window, cx| {
                        let data_grid_weak = data_grid_entity.downgrade();
                        let delegate_read = data_grid_entity.read(cx).table.read(cx);
                        let all_columns = delegate_read.delegate().columns().to_vec();
                        let pk_keys: HashSet<SharedString> = delegate_read
                            .delegate()
                            .primary_key_indices()
                            .iter()
                            .filter_map(|&i| {
                                delegate_read
                                    .delegate()
                                    .columns()
                                    .get(i)
                                    .map(|c| c.key.clone())
                            })
                            .collect();

                        all_columns
                            .into_iter()
                            .fold(menu, |menu, col| {
                                let is_pk = pk_keys.contains(&col.key);
                                let col_key = col.key.clone();
                                let col_name = col.name.clone();
                                let dg_weak = data_grid_weak.clone();
                                let dg_weak_for_click = data_grid_weak.clone();
                                let col_key_for_click = col_key.clone();
                                menu.item(
                                    PopupMenuItem::element(move |_window, cx| {
                                        let is_hidden = dg_weak
                                            .upgrade()
                                            .map(|dg| dg.read(cx).hidden_columns.contains(&col_key))
                                            .unwrap_or(false);
                                        let check_icon = if is_hidden {
                                            Icon::empty().xsmall()
                                        } else {
                                            Icon::new(IconName::Check)
                                                .xsmall()
                                                .text_color(cx.theme().success_foreground)
                                        };

                                        h_flex()
                                            .w_full()
                                            .gap_1()
                                            .items_center()
                                            .child(check_icon)
                                            .child(div().flex_1().child(col_name.clone()))
                                    })
                                    .on_click(
                                        move |_, _, cx| {
                                            if is_pk {
                                                return;
                                            }
                                            if let Some(dg) = dg_weak_for_click.upgrade() {
                                                dg.update(cx, |grid, cx| {
                                                    if grid
                                                        .hidden_columns
                                                        .contains(&col_key_for_click)
                                                    {
                                                        grid.hidden_columns
                                                            .remove(&col_key_for_click);
                                                    } else {
                                                        grid.hidden_columns
                                                            .insert(col_key_for_click.clone());
                                                    }
                                                    grid.apply_column_visibility(cx);
                                                    grid.save_column_visibility(cx);
                                                });
                                            }
                                        },
                                    ),
                                )
                            })
                            .separator()
                            .item(
                                PopupMenuItem::new(
                                    t!("TableDataGrid.reset_column_visibility").to_string(),
                                )
                                .on_click(move |_, _, cx| {
                                    if let Some(dg) = data_grid_weak.upgrade() {
                                        dg.update(cx, |grid, cx| {
                                            grid.hidden_columns.clear();
                                            grid.apply_column_visibility(cx);
                                            grid.save_column_visibility(cx);
                                        });
                                    }
                                }),
                            )
                            .keep_open(true)
                    })
            })
            .child(
                Button::new("toggle-editor")
                    .with_size(Size::Medium)
                    .icon(IconName::EditBorder)
                    .when(large_text_button_selected, |this| this.primary())
                    .tooltip(t!("TableDataGrid.large_text_editor").to_string())
                    .border_color(cx.theme().border.opacity(0.5))
                    .disabled(loading)
                    .on_click(cx.listener(Self::handle_large_text_editor)),
            )
            .child(
                Button::new("export-data")
                    .with_size(Size::Medium)
                    .icon(IconName::Export)
                    .tooltip(t!("TableDataGrid.export").to_string())
                    .border_color(cx.theme().border.opacity(0.5))
                    .disabled(loading)
                    .dropdown_menu(move |menu, window, _cx| {
                        menu.item(
                            PopupMenuItem::new(t!("TableDataGrid.export_result_xlsx").to_string())
                                .on_click(window.listener_for(
                                    &data_grid,
                                    |this, _, window, cx| {
                                        this.export_result_set(
                                            ExportScope::All,
                                            ExportFormat::Xlsx,
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .item(
                            PopupMenuItem::new(t!("TableDataGrid.export_result_csv").to_string())
                                .on_click(window.listener_for(
                                    &data_grid,
                                    |this, _, window, cx| {
                                        this.export_result_set(
                                            ExportScope::All,
                                            ExportFormat::Csv,
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .item(
                            PopupMenuItem::new(
                                t!("TableDataGrid.export_result_insert_sql").to_string(),
                            )
                            .on_click(window.listener_for(
                                &data_grid,
                                |this, _, window, cx| {
                                    this.export_result_set(
                                        ExportScope::All,
                                        ExportFormat::InsertSql,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new(
                                t!("TableDataGrid.export_current_page_xlsx").to_string(),
                            )
                            .on_click(window.listener_for(
                                &data_grid,
                                |this, _, window, cx| {
                                    this.export_result_set(
                                        ExportScope::CurrentPage,
                                        ExportFormat::Xlsx,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                        .item(
                            PopupMenuItem::new(
                                t!("TableDataGrid.export_current_page_csv").to_string(),
                            )
                            .on_click(window.listener_for(
                                &data_grid,
                                |this, _, window, cx| {
                                    this.export_result_set(
                                        ExportScope::CurrentPage,
                                        ExportFormat::Csv,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                        .item(
                            PopupMenuItem::new(
                                t!("TableDataGrid.export_current_page_insert_sql").to_string(),
                            )
                            .on_click(window.listener_for(
                                &data_grid,
                                |this, _, window, cx| {
                                    this.export_result_set(
                                        ExportScope::CurrentPage,
                                        ExportFormat::InsertSql,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    }),
            )
            .into_any_element()
    }

    pub fn render_table_area(&self, _window: &mut Window, cx: &App) -> AnyElement {
        let error_message = self.table_data_info.read(cx).error_message.clone();
        let panel_bg = layered_level_surface_color(
            cx.theme().background,
            cx.theme().window_blur_enabled,
            cx.theme().backdrop_opacity,
            1,
            WindowsSurfaceLayer::ContentBase,
        );
        let table_bg = layered_level_surface_color(
            cx.theme().background,
            cx.theme().window_blur_enabled,
            cx.theme().backdrop_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        );

        if let Some(error) = error_message {
            return div()
                .flex_1()
                .w_full()
                .h_full()
                .bg(panel_bg)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_color(cx.theme().danger_foreground)
                        .text_sm()
                        .child(error),
                )
                .into_any_element();
        }

        let table_view = EditTable::new(&self.table);
        div()
            .flex_1()
            .w_full()
            .h_full()
            .bg(table_bg)
            .border_1()
            .border_color(cx.theme().border)
            .child(table_view)
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &Context<Self>) -> AnyElement {
        let table_data_info = self.table_data_info.read(cx);
        let table = self.table.read(cx);

        let filtered_count = table.delegate().filtered_row_count();
        let total_rows = table.delegate().rows.len();
        let current_page_size = table_data_info.page_size;

        h_flex()
            .gap_3()
            .items_center()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(div().text_sm().text_color(cx.theme().foreground).child({
                if filtered_count < total_rows {
                    t!(
                        "TableDataGrid.page_info",
                        page_size = filtered_count,
                        page_count = total_rows,
                        total_count = table_data_info.total_count
                    )
                    .to_string()
                } else {
                    t!(
                        "TableDataGrid.page_number",
                        page = table_data_info.current_page,
                        total = table_data_info.total_count
                    )
                    .to_string()
                }
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        t!(
                            "TableDataGrid.query_elapsed",
                            duration = table_data_info.duration
                        )
                        .to_string(),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(table_data_info.current_sql.clone()),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("prev-page")
                            .with_size(Size::Small)
                            .icon(IconName::ChevronLeft)
                            .on_click(cx.listener(Self::handle_prev_page_click)),
                    )
                    .child({
                        let label = match current_page_size {
                            0 => t!("TableDataGrid.all").to_string(),
                            n => format!("{}", n),
                        };

                        Button::new("page-size-selector")
                            .with_size(Size::Small)
                            .label(label)
                            .dropdown_menu_with_anchor(Corner::TopRight, move |menu, _, _| {
                                menu.menu("100", Box::new(Page100))
                                    .menu("200", Box::new(Page200))
                                    .menu("300", Box::new(Page300))
                                    .menu("500", Box::new(Page500))
                                    .menu("1000", Box::new(Page1000))
                            })
                    })
                    .child(
                        Button::new("next-page")
                            .with_size(Size::Small)
                            .icon(IconName::ChevronRight)
                            .on_click(cx.listener(Self::handle_next_page_click)),
                    ),
            )
            .into_any_element()
    }

    fn render_simple_status_bar(&self, cx: &App) -> AnyElement {
        let row_count = self.config.rows_count;
        // 将SQL中的换行符替换为空格，保持单行显示
        let sql = self.config.sql.replace('\n', " ").replace('\r', " ");
        let execution_time = self.config.execution_time;

        h_flex()
            .gap_3()
            .items_center()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(t!("TableDataGrid.total_records", count = row_count).to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        t!("TableDataGrid.query_elapsed", duration = execution_time).to_string(),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(sql),
            )
            .into_any_element()
    }
}

impl Render for DataGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_table_data = self.config.usage == DataGridUsage::TableData;

        v_flex()
            .when(is_table_data, |this| {
                this.on_action(cx.listener(Self::handle_page_change_100))
                    .on_action(cx.listener(Self::handle_page_change_200))
                    .on_action(cx.listener(Self::handle_page_change_300))
                    .on_action(cx.listener(Self::handle_page_change_500))
                    .on_action(cx.listener(Self::handle_page_change_1000))
            })
            .track_focus(&self.focus_handle)
            .key_context(DB_SEARCH_CONTEXT)
            .on_action(cx.listener(Self::on_action_focus_search))
            .size_full()
            .gap_0()
            .child(self.render_toolbar(window, cx))
            .when(is_table_data, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .w_full()
                        .px_2()
                        .py_1()
                        .bg(cx.theme().background)
                        .child(self.filter_editor.clone()),
                )
            })
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(self.render_table_area(window, cx)),
            )
            .child(if is_table_data {
                self.render_status_bar(cx)
            } else {
                self.render_simple_status_bar(cx)
            })
    }
}

impl Clone for DataGrid {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            table: self.table.clone(),
            _table_sub: None,
            focus_handle: self.focus_handle.clone(),
            table_data_info: self.table_data_info.clone(),
            filter_editor: self.filter_editor.clone(),
            _filter_sub: None,
            search_input: self.search_input.clone(),
            _search_sub: None,
            hidden_columns: self.hidden_columns.clone(),
            column_visibility_loaded: self.column_visibility_loaded,
            is_large_text_editor_sidebar_open: self.is_large_text_editor_sidebar_open,
            self_entity: self.self_entity.clone(),
        }
    }
}

impl EventEmitter<DataGridEvent> for DataGrid {}

impl Focusable for DataGrid {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[inline]
pub fn notification(cx: &mut App, error: String) {
    if let Some(window) = cx.active_window() {
        _ = window.update(cx, |_, w, cx| w.push_notification(error, cx));
    };
}

#[cfg(test)]
mod tests {
    use super::{
        ExportFormat, LargeTextEditorRoute, TableMetadata, build_header_order_by_clause,
        build_large_text_editor_title, collect_delete_row_indices, resolve_large_text_editor_route,
    };
    use crate::settings::LargeTextEditorOpenMode;
    use db::DbManager;
    use gpui::SharedString;
    use one_core::storage::DatabaseType;
    use one_ui::edit_table::ColumnSort;
    use rust_i18n::t;
    use std::io::{Cursor, Read};
    use zip::ZipArchive;

    fn sample_export_input() -> (Vec<Vec<Option<String>>>, Vec<SharedString>, TableMetadata) {
        let rows = vec![vec![
            Some("1".to_string()),
            Some("Line 1\nLine 2".to_string()),
        ]];
        let columns = vec![SharedString::from("id"), SharedString::from("body")];
        let metadata = TableMetadata::new("news").with_columns(vec!["id", "body"]);
        (rows, columns, metadata)
    }

    fn read_xlsx_entry(bytes: &[u8], entry_name: &str) -> String {
        let mut archive =
            ZipArchive::new(Cursor::new(bytes)).expect("xlsx bytes should be a valid zip archive");
        let mut entry = archive
            .by_name(entry_name)
            .unwrap_or_else(|_| panic!("missing xlsx entry: {entry_name}"));
        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .expect("xlsx entry should be valid utf-8 xml");
        content
    }

    #[test]
    fn build_header_order_by_clause_quotes_mysql_identifier() {
        let clause = build_header_order_by_clause(
            &DbManager::default(),
            DatabaseType::MySQL,
            "order",
            ColumnSort::Descending,
        )
        .expect("should build order by clause");

        assert_eq!(clause.as_deref(), Some("`order` DESC"));
    }

    #[test]
    fn build_header_order_by_clause_quotes_postgresql_identifier() {
        let clause = build_header_order_by_clause(
            &DbManager::default(),
            DatabaseType::PostgreSQL,
            "created_at",
            ColumnSort::Ascending,
        )
        .expect("should build order by clause");

        assert_eq!(clause.as_deref(), Some("\"created_at\" ASC"));
    }

    #[test]
    fn build_header_order_by_clause_clears_default_sort() {
        let clause = build_header_order_by_clause(
            &DbManager::default(),
            DatabaseType::SQLite,
            "id",
            ColumnSort::Default,
        )
        .expect("default sort should not require a clause");

        assert!(clause.is_none());
    }

    #[test]
    fn collect_delete_row_indices_dedups_and_sorts_descending() {
        let rows = collect_delete_row_indices(vec![2, 4, 2, 1, 4], None);

        assert_eq!(rows, vec![4, 2, 1]);
    }

    #[test]
    fn collect_delete_row_indices_uses_fallback_when_selection_is_empty() {
        let rows = collect_delete_row_indices(Vec::<usize>::new(), Some(3));

        assert_eq!(rows, vec![3]);
    }

    #[test]
    fn collect_delete_row_indices_prefers_selection_over_fallback() {
        let rows = collect_delete_row_indices(vec![1, 0], Some(5));

        assert_eq!(rows, vec![1, 0]);
    }

    #[test]
    fn build_large_text_editor_title_formats_row_and_column() {
        let title = build_large_text_editor_title("payload", 2);

        assert_eq!(
            title,
            t!("TableDataGrid.edit_cell_title", column = "payload", row = 3).to_string()
        );
    }

    #[test]
    fn resolve_large_text_editor_route_uses_page_preview_for_sidebar_mode() {
        let route = resolve_large_text_editor_route(LargeTextEditorOpenMode::SidebarPreview);

        assert_eq!(route, LargeTextEditorRoute::PagePreview);
    }

    #[test]
    fn resolve_large_text_editor_route_uses_dialog_for_dialog_mode() {
        let route = resolve_large_text_editor_route(LargeTextEditorOpenMode::Dialog);

        assert_eq!(route, LargeTextEditorRoute::Dialog);
    }

    #[test]
    fn build_export_bytes_creates_zip_based_xlsx_payload() {
        let (rows, columns, metadata) = sample_export_input();

        let bytes = super::build_export_bytes(ExportFormat::Xlsx, rows, columns, metadata)
            .expect("xlsx export should build successfully")
            .expect("xlsx export should produce bytes");

        assert!(bytes.starts_with(b"PK"));
        let sheet_xml = read_xlsx_entry(&bytes, "xl/worksheets/sheet1.xml");
        assert!(sheet_xml.contains("<sheetData>"));
    }

    #[test]
    fn build_export_bytes_preserves_newlines_in_xlsx_cells() {
        let (rows, columns, metadata) = sample_export_input();

        let bytes = super::build_export_bytes(ExportFormat::Xlsx, rows, columns, metadata)
            .expect("xlsx export should build successfully")
            .expect("xlsx export should produce bytes");

        let shared_strings = read_xlsx_entry(&bytes, "xl/sharedStrings.xml");
        assert!(
            shared_strings.contains("Line 1\nLine 2")
                || shared_strings.contains("Line 1&#10;Line 2")
                || shared_strings.contains("Line 1&#xA;Line 2")
        );
    }
}
