use crate::database_view_plugin::{DatabaseViewPluginRegistry, ToolbarButtonType};
use crate::db_tree_view::get_icon_for_node_type;
use db::{DbNode, DbNodeType, GlobalDbState, ObjectView};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Stateful,
    Styled, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::label::Label;
use gpui_component::notification::Notification;
use gpui_component::table::{Table, TableDelegate, TableEvent, TableState};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Size, WindowsSurfaceLayer, h_flex,
    layered_level_surface_color, table::Column, v_flex,
};
use one_core::storage::manager::get_queries_dir;
use one_core::storage::{
    ConnectionRepository, DatabaseType, DbConnectionConfig, GlobalStorageState, StorageManager,
    Workspace,
};
use one_core::tab_container::{TabContent, TabContentEvent};
use one_core::utils::debouncer::Debouncer;
use rust_i18n::t;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

fn format_timestamp(ts: i64) -> String {
    use chrono::{DateTime, Local};
    if let Some(dt) = DateTime::from_timestamp_millis(ts) {
        let local: DateTime<Local> = dt.into();
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "".to_string()
    }
}

/// 数据库对象面板事件 - 统一的表格交互事件
#[derive(Clone, Debug)]
pub enum DatabaseObjectsEvent {
    /// 刷新当前视图
    Refresh { node: DbNode },

    /// 将数据库添加到树视图并展开
    AddDatabaseToTree { node: DbNode },

    /// 新建数据库
    CreateDatabase { node: DbNode },

    /// 编辑数据库
    EditDatabase { node: DbNode },

    /// 删除数据库
    DeleteDatabase { node: DbNode },

    /// 删除连接
    DeleteConnection { node: DbNode },

    /// 关闭连接
    CloseConnection { node: DbNode },

    /// 打开表数据
    OpenTableData { node: DbNode },

    /// 设计表（新建或编辑）
    DesignTable { node: DbNode },

    /// 删除表
    DeleteTable { node: DbNode },

    /// 打开视图数据
    OpenViewData { node: DbNode },

    /// 删除视图
    DeleteView { node: DbNode },

    /// 新建查询
    CreateNewQuery { node: DbNode },

    /// 打开命名查询
    OpenNamedQuery { node: DbNode },

    /// 重命名查询
    RenameQuery { node: DbNode },

    /// 删除查询
    DeleteQuery { node: DbNode },

    /// 删除模式/Schema
    DeleteSchema { node: DbNode },

    /// 新建模式/Schema
    CreateSchema { node: DbNode },

    /// 批量操作
    Batch {
        action: DatabaseObjectsBatchAction,
        nodes: Vec<DbNode>,
    },
}

#[derive(Clone, Debug)]
pub enum DatabaseObjectsBatchAction {
    DeleteConnection,
    DeleteDatabase,
    DeleteSchema,
    DeleteTable,
    DeleteView,
    DeleteQuery,
}

/// Table delegate for DatabaseObjects table view
#[derive(Clone)]
pub struct DatabaseObjectsTableDelegate {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<String>>,
    pub filtered_rows: Vec<usize>,
    pub db_node_type: DbNodeType,
}

impl Default for DatabaseObjectsTableDelegate {
    fn default() -> Self {
        Self {
            columns: vec![],
            rows: vec![],
            filtered_rows: vec![],
            db_node_type: DbNodeType::default(),
        }
    }
}

impl TableDelegate for DatabaseObjectsTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len() + 1
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.filtered_rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        if col_ix == 0 {
            Column::new("#", "#").width(px(48.)).resizable(false)
        } else {
            self.columns
                .get(col_ix - 1)
                .cloned()
                .unwrap_or_else(|| Column::new("col", "col"))
        }
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let column = self.column(col_ix, cx);
        let is_last = col_ix == self.columns_count(cx) - 1;

        div()
            .when(!is_last, |el| el.w(column.width))
            .when(is_last, |el| el.flex_1())
            .h_full()
            .px_2()
            .flex()
            .items_center()
            .text_sm()
            .text_color(cx.theme().table_head_foreground)
            .child(column.name.clone())
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div().id(row_ix)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(&original_row) = self.filtered_rows.get(row_ix) else {
            return div().size_full().into_any_element();
        };
        let Some(row_values) = self.rows.get(original_row) else {
            return div().size_full().into_any_element();
        };

        if col_ix == 0 {
            return div()
                .size_full()
                .px_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child((row_ix + 1).to_string())
                .into_any_element();
        }

        let data_col_ix = col_ix - 1;
        let cell_value = row_values.get(data_col_ix).cloned().unwrap_or_default();

        if data_col_ix == 0 {
            let icon = get_icon_for_node_type(&self.db_node_type, cx.theme()).color();
            h_flex()
                .size_full()
                .gap_2()
                .items_center()
                .child(icon)
                .child(Label::new(cell_value))
                .into_any_element()
        } else {
            div().size_full().child(cell_value).into_any_element()
        }
    }
}

pub struct DatabaseObjects {
    loaded_data: Entity<ObjectView>,
    // 直接管理表格数据
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    filtered_rows: Vec<usize>,
    db_node_type: DbNodeType,
    focus_handle: FocusHandle,
    workspace: Option<Workspace>,
    search_input: Entity<InputState>,
    search_query: String,
    search_seq: u64,
    search_debouncer: Arc<Debouncer>,
    current_node: Option<DbNode>,
    // 用于批量操作时的多选记录
    selected_indices: HashSet<usize>,
    // Table 状态
    table: Entity<TableState<DatabaseObjectsTableDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl DatabaseObjects {
    pub fn new(workspace: Option<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let loaded_data = cx.new(|_| ObjectView::default());
        let focus_handle = cx.focus_handle();
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("Common.search"))
                .clean_on_escape()
        });
        let search_debouncer = Arc::new(Debouncer::new(Duration::from_millis(250)));

        let search_sub = cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self,
             input: &Entity<InputState>,
             event: &InputEvent,
             _window,
             cx: &mut Context<Self>| {
                if let InputEvent::Change = event {
                    let query = input.read(cx).text().to_string();

                    this.search_seq += 1;
                    let current_seq = this.search_seq;
                    let debouncer = Arc::clone(&this.search_debouncer);
                    let query_for_task = query.clone();

                    cx.spawn(async move |view, cx| {
                        if debouncer.debounce(cx).await {
                            _ = view.update(cx, |this, cx| {
                                if this.search_seq == current_seq {
                                    this.search_query = query_for_task.to_lowercase();
                                    this.selected_indices.clear();
                                    this.apply_filter();
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .detach();
                }
            },
        );

        let storage_manager = cx.global::<GlobalStorageState>().storage.clone();
        let clone_workspace = workspace.clone();
        cx.spawn(async move |entity: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = Self::load_connection_list_view(storage_manager, clone_workspace);
            if let Some(view) = result {
                let columns = view.columns.clone();
                let rows = view.rows.clone();
                let db_node_type = view.db_node_type.clone();
                entity
                    .update(cx, move |this, cx| {
                        this.loaded_data.update(cx, |data, _cx| {
                            *data = view;
                        });
                        this.columns = columns;
                        this.rows = rows;
                        this.filtered_rows = (0..this.rows.len()).collect();
                        this.db_node_type = db_node_type;
                        this.selected_indices.clear();
                        this.sync_table_delegate(cx);
                        cx.emit(TabContentEvent::StateChanged);
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();

        // Create TableState with default delegate
        let table_delegate = DatabaseObjectsTableDelegate::default();
        let table = cx.new(|cx| {
            TableState::new(table_delegate, window, cx)
                .row_selectable(true)
                .cell_selectable(false)
        });

        // Subscribe to table double-click events
        let table_sub = cx.subscribe_in(
            &table,
            window,
            move |this: &mut DatabaseObjects,
                  _: &Entity<TableState<DatabaseObjectsTableDelegate>>,
                  event: &TableEvent,
                  _window,
                  cx| {
                if let TableEvent::DoubleClickedRow(row_ix) = event {
                    this.handle_row_double_click(*row_ix, cx);
                }
            },
        );

        Self {
            loaded_data,
            columns: vec![],
            rows: vec![],
            filtered_rows: vec![],
            db_node_type: DbNodeType::default(),
            focus_handle,
            workspace,
            search_input,
            search_query: "".to_string(),
            search_seq: 0,
            search_debouncer,
            current_node: None,
            selected_indices: HashSet::new(),
            table,
            _subscriptions: vec![search_sub, table_sub],
        }
    }

    fn handle_row_double_click(&self, row: usize, cx: &mut Context<Self>) {
        let Some(node) = self.build_node_for_row(row) else {
            return;
        };

        let event = match node.node_type {
            DbNodeType::Table => DatabaseObjectsEvent::OpenTableData { node },
            DbNodeType::View => DatabaseObjectsEvent::OpenViewData { node },
            DbNodeType::NamedQuery => DatabaseObjectsEvent::OpenNamedQuery { node },
            DbNodeType::Database => DatabaseObjectsEvent::AddDatabaseToTree { node },
            _ => return,
        };

        cx.emit(event);
    }

    pub fn handle_node_selected(
        &mut self,
        node: DbNode,
        _config: DbConnectionConfig,
        cx: &mut Context<Self>,
    ) {
        match node.node_type {
            DbNodeType::Connection
            | DbNodeType::Database
            | DbNodeType::Schema
            | DbNodeType::TablesFolder
            | DbNodeType::Table
            | DbNodeType::ViewsFolder
            | DbNodeType::View
            | DbNodeType::QueriesFolder
            | DbNodeType::NamedQuery => {}
            _ => return,
        }

        if !node.children_loaded && node.node_type != DbNodeType::Connection {
            return;
        }

        self.current_node = Some(node.clone());
        let node_clone = node.clone();
        let storage_manager = cx.global::<GlobalStorageState>().storage.clone();
        let global_state = cx.global::<GlobalDbState>().clone();
        let workspace = self.workspace.clone();
        let connection_id = node.connection_id.clone();
        cx.spawn(async move |entity: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result: Option<ObjectView> =
                if !node_clone.children_loaded && node_clone.node_type == DbNodeType::Connection {
                    Self::load_connection_list_view(storage_manager, workspace)
                } else if node_clone.node_type == DbNodeType::QueriesFolder
                    || node_clone.node_type == DbNodeType::NamedQuery
                {
                    Self::load_queries_list_view(node_clone.clone()).await
                } else {
                    global_state
                        .load_object_view(cx, connection_id, node_clone)
                        .await
                        .ok()
                        .flatten()
                };

            if let Some(view) = result {
                let columns = view.columns.clone();
                let rows = view.rows.clone();
                let db_node_type = view.db_node_type.clone();
                entity
                    .update(cx, move |this, cx| {
                        let search_query = this.search_query.clone();
                        this.loaded_data.update(cx, |data, _cx| {
                            *data = view;
                        });
                        this.columns = columns;
                        this.rows = rows;
                        this.db_node_type = db_node_type;
                        if !search_query.is_empty() {
                            this.apply_filter();
                        } else {
                            this.filtered_rows = (0..this.rows.len()).collect();
                        }
                        this.selected_indices.clear();
                        this.sync_table_delegate(cx);
                        cx.emit(TabContentEvent::StateChanged);
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    }

    /// 同步 Table delegate 数据并刷新列组
    fn sync_table_delegate(&mut self, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().columns = self.columns.clone();
            state.delegate_mut().rows = self.rows.clone();
            state.delegate_mut().filtered_rows = self.filtered_rows.clone();
            state.delegate_mut().db_node_type = self.db_node_type.clone();
            state.refresh(cx);
        });
    }

    fn apply_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_rows = (0..self.rows.len()).collect();
        } else {
            self.filtered_rows = self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    row.iter()
                        .any(|cell| cell.to_lowercase().contains(&self.search_query))
                })
                .map(|(idx, _)| idx)
                .collect();
        }
    }

    fn load_connection_list_view(
        storage_manager: StorageManager,
        workspace: Option<Workspace>,
    ) -> Option<ObjectView> {
        let conn_repo = storage_manager.get::<ConnectionRepository>()?;
        let w = workspace?;
        let connections = conn_repo.list_by_workspace(w.id).ok()?;

        let rows = connections
            .iter()
            .map(|stored_conn| {
                let created = stored_conn
                    .created_at
                    .map(|ts| format_timestamp(ts))
                    .unwrap_or_default();
                let updated = stored_conn
                    .updated_at
                    .map(|ts| format_timestamp(ts))
                    .unwrap_or_default();
                let remark = stored_conn.remark.clone().unwrap_or_default();
                let db_type = stored_conn
                    .to_db_connection()
                    .map(|c| c.database_type)
                    .unwrap_or(DatabaseType::MySQL);
                let connection_id = stored_conn.id.map(|id| id.to_string()).unwrap_or_default();
                vec![
                    stored_conn.name.clone(),
                    connection_id,
                    db_type.as_str().into(),
                    created,
                    updated,
                    remark,
                ]
            })
            .collect();

        Some(ObjectView {
            db_node_type: DbNodeType::Connection,
            columns: vec![
                Column::new("name", t!("ConnectionForm.connection_name")).width(200.0),
                Column::new("id", "ID").width(80.0),
                Column::new("type", t!("Common.type")),
                Column::new("created_at", t!("Table.created_at")).width(200.0),
                Column::new("updated_at", t!("Table.updated_at")).width(200.0),
                Column::new("remark", t!("ConnectionForm.remark")).width(250.0),
            ],
            rows,
            title: t!("Connection.connection_list").to_string(),
        })
    }

    async fn load_queries_list_view(node: DbNode) -> Option<ObjectView> {
        use std::time::UNIX_EPOCH;

        let database_name = node.get_database_name().unwrap_or_default();
        let database_type = node.database_type.as_str();
        let connection_id = node.connection_id.clone();

        let queries_dir = get_queries_dir().ok()?;
        let query_path = queries_dir
            .join(database_type)
            .join(&connection_id)
            .join(&database_name);

        if !query_path.exists() {
            return Some(ObjectView {
                db_node_type: DbNodeType::NamedQuery,
                columns: vec![
                    Column::new("name", t!("Query.query_name")).width(200.0),
                    Column::new("created_at", t!("Table.created_at")).width(180.0),
                    Column::new("updated_at", t!("Table.updated_at")).width(180.0),
                ],
                rows: vec![],
                title: t!("Query.query_list").to_string(),
            });
        }

        let entries = std::fs::read_dir(&query_path).ok()?;
        let mut rows: Vec<Vec<String>> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "sql") {
                let file_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let (created, modified) = if let Ok(metadata) = std::fs::metadata(&path) {
                    let created_time = metadata
                        .created()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| format_timestamp(d.as_millis() as i64))
                        .unwrap_or_default();
                    let modified_time = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| format_timestamp(d.as_millis() as i64))
                        .unwrap_or_default();
                    (created_time, modified_time)
                } else {
                    (String::new(), String::new())
                };

                rows.push(vec![file_name, created, modified]);
            }
        }

        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        Some(ObjectView {
            db_node_type: DbNodeType::NamedQuery,
            columns: vec![
                Column::new("name", t!("Query.query_name")).width(200.0),
                Column::new("created_at", t!("Table.created_at")).width(180.0),
                Column::new("updated_at", t!("Table.updated_at")).width(180.0),
            ],
            rows,
            title: t!("Query.query_list").to_string(),
        })
    }

    fn build_node_for_row(&self, row_ix: usize) -> Option<DbNode> {
        let db_node_type = self.db_node_type.clone();

        let original_row = self.filtered_rows.get(row_ix).copied()?;
        let row_data = self.rows.get(original_row)?;
        let name = row_data.first().cloned()?;

        // 特殊处理：当 current_node 为 None 且显示连接列表时
        if self.current_node.is_none() && db_node_type == DbNodeType::Connection {
            let connection_id = row_data.get(1).cloned().unwrap_or_default();
            let db_type_str = row_data.get(2).cloned().unwrap_or_default();
            let database_type = DatabaseType::from_str(&db_type_str).unwrap_or(DatabaseType::MySQL);

            return Some(DbNode::new(
                connection_id.clone(),
                name,
                DbNodeType::Connection,
                connection_id,
                database_type,
            ));
        }

        let current_node = self.current_node.as_ref()?;
        let connection_id = current_node.connection_id.clone();
        let database_type = current_node.database_type;

        let mut metadata: HashMap<String, String> = current_node.metadata.clone();
        let database = metadata.get("database").cloned().unwrap_or_default();

        let (node_id, target_node_type) = match db_node_type {
            DbNodeType::Connection => {
                if current_node.children_loaded {
                    metadata.insert("database".to_string(), name.clone());
                    (format!("{}:{}", connection_id, name), DbNodeType::Database)
                } else {
                    // 连接未展开时，从行数据获取 connection_id
                    let row_connection_id = row_data.get(1).cloned().unwrap_or_default();
                    let db_type_str = row_data.get(2).cloned().unwrap_or_default();
                    let row_database_type =
                        DatabaseType::from_str(&db_type_str).unwrap_or(DatabaseType::MySQL);
                    return Some(DbNode::new(
                        row_connection_id.clone(),
                        name,
                        DbNodeType::Connection,
                        row_connection_id,
                        row_database_type,
                    ));
                }
            }
            DbNodeType::Database => {
                if current_node.node_type == DbNodeType::Connection {
                    metadata.insert("database".to_string(), name.clone());
                    (format!("{}:{}", connection_id, name), DbNodeType::Database)
                } else {
                    let db = if database.is_empty() {
                        current_node.name.clone()
                    } else {
                        database.clone()
                    };
                    metadata.insert("database".to_string(), db.clone());
                    metadata.insert("table".to_string(), name.clone());
                    (
                        format!("{}:{}:table_folder:{}", connection_id, db, name),
                        DbNodeType::Table,
                    )
                }
            }
            DbNodeType::TablesFolder | DbNodeType::Table => {
                let db = if database.is_empty() {
                    current_node.name.clone()
                } else {
                    database.clone()
                };
                metadata.insert("database".to_string(), db.clone());
                metadata.insert("table".to_string(), name.clone());
                (
                    format!("{}:{}:table_folder:{}", connection_id, db, name),
                    DbNodeType::Table,
                )
            }
            DbNodeType::Schema => {
                if current_node.node_type == DbNodeType::Connection {
                    metadata.insert("schema".to_string(), name.clone());
                    (format!("{}:{}", connection_id, name), DbNodeType::Schema)
                } else {
                    let db = metadata
                        .get("database")
                        .cloned()
                        .unwrap_or_else(|| current_node.name.clone());
                    let schema = current_node.name.clone();
                    metadata.insert("database".to_string(), db.clone());
                    metadata.insert("schema".to_string(), schema.clone());
                    metadata.insert("table".to_string(), name.clone());
                    (
                        format!("{}:{}:{}:table_folder:{}", connection_id, db, schema, name),
                        DbNodeType::Table,
                    )
                }
            }
            DbNodeType::ViewsFolder | DbNodeType::View => {
                let db = if database.is_empty() {
                    current_node.name.clone()
                } else {
                    database.clone()
                };
                metadata.insert("database".to_string(), db.clone());
                metadata.insert("view".to_string(), name.clone());
                (
                    format!("{}:{}:views_folder:{}", connection_id, db, name),
                    DbNodeType::View,
                )
            }
            DbNodeType::QueriesFolder | DbNodeType::NamedQuery => {
                let query_id = row_data.get(1).cloned().unwrap_or_default();
                metadata.insert("query_name".to_string(), name.clone());
                metadata.insert("query_id".to_string(), query_id.clone());

                // 重建 file_path 以支持双击打开查询
                if let Ok(queries_dir) = get_queries_dir() {
                    let db_name = if database.is_empty() {
                        current_node.get_database_name().unwrap_or_default()
                    } else {
                        database.clone()
                    };
                    let file_path = queries_dir
                        .join(database_type.as_str())
                        .join(&connection_id)
                        .join(&db_name)
                        .join(format!("{}.sql", name));
                    if let Some(path_str) = file_path.to_str() {
                        metadata.insert("file_path".to_string(), path_str.to_string());
                    }
                }

                (
                    format!("{}:queries:{}", connection_id, query_id),
                    DbNodeType::NamedQuery,
                )
            }
            _ => return None,
        };

        Some(
            DbNode::new(
                node_id,
                name,
                target_node_type,
                connection_id,
                database_type,
            )
            .with_metadata(metadata),
        )
    }

    fn build_nodes_for_selected_rows(&self) -> Vec<DbNode> {
        let mut selected_rows: Vec<usize> = self.selected_indices.iter().copied().collect();
        selected_rows.sort_unstable();
        selected_rows
            .into_iter()
            .filter_map(|row_ix| self.build_node_for_row(row_ix))
            .collect()
    }
}

impl DatabaseObjects {
    fn tab_title_key(db_node_type: DbNodeType) -> &'static str {
        match db_node_type {
            DbNodeType::Connection => "Connection.connection_list",
            DbNodeType::Database => "Database.database",
            DbNodeType::Schema => "Schema.schema",
            DbNodeType::TablesFolder | DbNodeType::Table => "DbTree.Tables",
            DbNodeType::ColumnsFolder | DbNodeType::Column => "DbTree.Columns",
            DbNodeType::IndexesFolder | DbNodeType::Index => "DbTree.Indexes",
            DbNodeType::ForeignKeysFolder | DbNodeType::ForeignKey => "DbTree.ForeignKeys",
            DbNodeType::TriggersFolder | DbNodeType::Trigger => "DbTree.Triggers",
            DbNodeType::ChecksFolder | DbNodeType::Check => "DbTree.Checks",
            DbNodeType::ViewsFolder | DbNodeType::View => "DbTree.Views",
            DbNodeType::FunctionsFolder | DbNodeType::Function => "DbTree.Functions",
            DbNodeType::ProceduresFolder | DbNodeType::Procedure => "DbTree.Procedures",
            DbNodeType::SequencesFolder | DbNodeType::Sequence => "DbTree.Sequences",
            DbNodeType::QueriesFolder | DbNodeType::NamedQuery => "Query.query_list",
        }
    }

    fn tab_title_base(db_node_type: DbNodeType) -> String {
        t!(Self::tab_title_key(db_node_type)).to_string()
    }

    fn should_show_database_name_in_title(db_node_type: DbNodeType) -> bool {
        matches!(
            db_node_type,
            DbNodeType::Table | DbNodeType::View | DbNodeType::NamedQuery
        )
    }

    fn compose_tab_title(
        db_node_type: DbNodeType,
        base_title: &str,
        database_name: Option<&str>,
    ) -> String {
        if !Self::should_show_database_name_in_title(db_node_type) {
            return base_title.to_string();
        }

        let database_name = database_name.map(str::trim).filter(|name| !name.is_empty());

        match database_name {
            Some(database_name) => format!("{}@{}", base_title, database_name),
            None => base_title.to_string(),
        }
    }

    fn tab_title(&self) -> String {
        let base_title = Self::tab_title_base(self.db_node_type);
        let database_name = self
            .current_node
            .as_ref()
            .and_then(|node| node.get_database_name());

        Self::compose_tab_title(self.db_node_type, &base_title, database_name.as_deref())
    }

    fn tab_width_size_for(db_node_type: DbNodeType) -> Size {
        if Self::should_show_database_name_in_title(db_node_type) {
            Size::Medium
        } else {
            Size::XSmall
        }
    }

    fn tab_width_size(&self) -> Size {
        Self::tab_width_size_for(self.db_node_type)
    }

    fn batch_action_for_event(event: &DatabaseObjectsEvent) -> Option<DatabaseObjectsBatchAction> {
        match event {
            DatabaseObjectsEvent::DeleteConnection { .. } => {
                Some(DatabaseObjectsBatchAction::DeleteConnection)
            }
            DatabaseObjectsEvent::DeleteDatabase { .. } => {
                Some(DatabaseObjectsBatchAction::DeleteDatabase)
            }
            DatabaseObjectsEvent::DeleteSchema { .. } => {
                Some(DatabaseObjectsBatchAction::DeleteSchema)
            }
            DatabaseObjectsEvent::DeleteTable { .. } => {
                Some(DatabaseObjectsBatchAction::DeleteTable)
            }
            DatabaseObjectsEvent::DeleteView { .. } => Some(DatabaseObjectsBatchAction::DeleteView),
            DatabaseObjectsEvent::DeleteQuery { .. } => {
                Some(DatabaseObjectsBatchAction::DeleteQuery)
            }
            _ => None,
        }
    }

    fn allow_multi_event(event: &DatabaseObjectsEvent) -> bool {
        matches!(
            event,
            DatabaseObjectsEvent::OpenTableData { .. }
                | DatabaseObjectsEvent::OpenViewData { .. }
                | DatabaseObjectsEvent::OpenNamedQuery { .. }
                | DatabaseObjectsEvent::DesignTable { .. }
                | DatabaseObjectsEvent::CloseConnection { .. }
        )
    }

    fn render_toolbar_buttons(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut buttons: Vec<AnyElement> = vec![];
        let current_node = self.current_node.clone();
        let data_db_node_type = self.db_node_type;
        let node_type = current_node
            .as_ref()
            .map(|n| n.node_type.clone())
            .unwrap_or(DbNodeType::Connection);
        let database_type = current_node
            .as_ref()
            .map(|n| n.database_type)
            .unwrap_or(DatabaseType::MySQL);

        buttons.push({
            let node = current_node.clone();
            Button::new("refresh-data")
                .ghost()
                .with_size(Size::Medium)
                .icon(IconName::Refresh)
                .tooltip(t!("Common.refresh"))
                .on_click(window.listener_for(&cx.entity(), move |_this, _, _, cx| {
                    if let Some(ref node) = node {
                        cx.emit(DatabaseObjectsEvent::Refresh { node: node.clone() });
                    }
                }))
                .into_any_element()
        });

        let plugin_registry = cx.global::<DatabaseViewPluginRegistry>();
        if let Some(plugin) = plugin_registry.get(&database_type) {
            let toolbar_buttons = plugin.build_toolbar_buttons(node_type, data_db_node_type);

            for btn_config in toolbar_buttons {
                let button = match btn_config.button_type {
                    ToolbarButtonType::CurrentNode => {
                        let node = current_node.clone();
                        let event_fn = btn_config.event_fn;
                        Button::new(btn_config.id)
                            .ghost()
                            .with_size(Size::Medium)
                            .icon(btn_config.icon)
                            .tooltip(btn_config.tooltip)
                            .on_click(window.listener_for(&cx.entity(), move |_this, _, _, cx| {
                                if let Some(ref node) = node {
                                    let event = event_fn(node.clone());
                                    cx.emit(event);
                                }
                            }))
                            .into_any_element()
                    }
                    ToolbarButtonType::SelectedRow => {
                        let event_fn = btn_config.event_fn;
                        Button::new(btn_config.id)
                            .ghost()
                            .with_size(Size::Medium)
                            .icon(btn_config.icon)
                            .tooltip(btn_config.tooltip)
                            .on_click(window.listener_for(
                                &cx.entity(),
                                move |this, _, window, cx| {
                                    let nodes = this.build_nodes_for_selected_rows();
                                    if nodes.is_empty() {
                                        window.push_notification(
                                            Notification::warning(t!("Common.select_row")),
                                            cx,
                                        );
                                        return;
                                    }
                                    if nodes.len() == 1 {
                                        let event = event_fn(nodes[0].clone());
                                        cx.emit(event);
                                        return;
                                    }

                                    let sample_event = event_fn(nodes[0].clone());
                                    if let Some(action) =
                                        Self::batch_action_for_event(&sample_event)
                                    {
                                        cx.emit(DatabaseObjectsEvent::Batch { action, nodes });
                                        return;
                                    }

                                    if !Self::allow_multi_event(&sample_event) {
                                        window.push_notification(
                                            Notification::warning(
                                                t!("DatabaseObjects.batch_not_supported")
                                                    .to_string(),
                                            ),
                                            cx,
                                        );
                                        return;
                                    }

                                    for node in nodes {
                                        let event = event_fn(node);
                                        cx.emit(event);
                                    }
                                },
                            ))
                            .into_any_element()
                    }
                };
                buttons.push(button);
            }
        }

        buttons
    }
}

impl Render for DatabaseObjects {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loaded_data = self.loaded_data.read(cx);
        let title = loaded_data.title.clone();
        let toolbar_buttons = self.render_toolbar_buttons(window, cx);
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let toolbar_bg = layered_level_surface_color(
            cx.theme().background,
            blur_enabled,
            window_opacity,
            0.14,
            WindowsSurfaceLayer::ContentSection,
        );

        // Update delegate with current data (no refresh here, only when data actually changes)
        self.table.update(cx, |state, _cx| {
            state.delegate_mut().columns = self.columns.clone();
            state.delegate_mut().rows = self.rows.clone();
            state.delegate_mut().filtered_rows = self.filtered_rows.clone();
            state.delegate_mut().db_node_type = self.db_node_type.clone();
        });

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(toolbar_bg)
                    .children(toolbar_buttons)
                    .child(div().flex_1())
                    .child({
                        div().flex_1().child(
                            Input::new(&self.search_input)
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .cleanable(true)
                                .small()
                                .w_full(),
                        )
                    })
                    .into_any_element(),
            )
            .child(
                div().flex_1().overflow_hidden().child(
                    Table::new(&self.table)
                        .bordered(true)
                        .with_size(Size::XSmall),
                ),
            )
            .child(
                div()
                    .p_2()
                    .text_sm()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .bg(toolbar_bg)
                    .child(title),
            )
    }
}

impl Clone for DatabaseObjects {
    fn clone(&self) -> Self {
        Self {
            loaded_data: self.loaded_data.clone(),
            columns: self.columns.clone(),
            rows: self.rows.clone(),
            filtered_rows: self.filtered_rows.clone(),
            db_node_type: self.db_node_type.clone(),
            focus_handle: self.focus_handle.clone(),
            workspace: self.workspace.clone(),
            search_input: self.search_input.clone(),
            search_seq: self.search_seq,
            search_query: self.search_query.clone(),
            search_debouncer: self.search_debouncer.clone(),
            current_node: self.current_node.clone(),
            selected_indices: self.selected_indices.clone(),
            table: self.table.clone(),
            _subscriptions: vec![],
        }
    }
}

impl EventEmitter<DatabaseObjectsEvent> for DatabaseObjects {}
impl EventEmitter<TabContentEvent> for DatabaseObjects {}

impl Focusable for DatabaseObjects {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub struct DatabaseObjectsPanel {
    database_objects: Entity<DatabaseObjects>,
    _subscriptions: Vec<Subscription>,
}

impl DatabaseObjectsPanel {
    pub fn new(workspace: Option<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let database_objects = cx.new(|cx| DatabaseObjects::new(workspace, window, cx));
        let state_sub = cx.subscribe(&database_objects, |_this, _, _: &TabContentEvent, cx| {
            cx.emit(TabContentEvent::StateChanged);
        });

        Self {
            database_objects,
            _subscriptions: vec![state_sub],
        }
    }

    pub fn database_objects(&self) -> &Entity<DatabaseObjects> {
        &self.database_objects
    }

    pub fn handle_node_selected(&self, node: DbNode, config: DbConnectionConfig, cx: &mut App) {
        self.database_objects.update(cx, |database_objects, cx| {
            database_objects.handle_node_selected(node, config, cx);
        })
    }

    pub fn refresh(&self, global_state: GlobalDbState, cx: &mut App) {
        self.database_objects.update(cx, |database_objects, cx| {
            if let Some(node) = database_objects.current_node.clone() {
                let connection_id = node.connection_id.clone();
                if let Some(config) = global_state.get_config(&connection_id) {
                    database_objects.handle_node_selected(node, config, cx);
                }
            }
        });
    }
}

impl EventEmitter<TabContentEvent> for DatabaseObjectsPanel {}

impl Render for DatabaseObjectsPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.database_objects.clone()
    }
}

impl Focusable for DatabaseObjectsPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.database_objects.focus_handle(cx)
    }
}

impl TabContent for DatabaseObjectsPanel {
    fn content_key(&self) -> &'static str {
        "DatabaseObjects"
    }

    fn title(&self, cx: &App) -> SharedString {
        let database_objects = self.database_objects.read(cx);
        let title = database_objects.tab_title();
        if !title.trim().is_empty() {
            return SharedString::from(title);
        }

        let loaded_title = database_objects.loaded_data.read(cx).title.clone();
        if !loaded_title.trim().is_empty() {
            return loaded_title.into();
        }

        SharedString::from(t!("DatabaseObjects.title"))
    }

    fn closeable(&self, _cx: &App) -> bool {
        false
    }

    fn width_size(&self, cx: &App) -> Option<Size> {
        let database_objects = self.database_objects.read(cx);
        Some(database_objects.tab_width_size())
    }
}

impl Clone for DatabaseObjectsPanel {
    fn clone(&self) -> Self {
        Self {
            database_objects: self.database_objects.clone(),
            _subscriptions: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DatabaseObjects;
    use db::DbNodeType;
    use gpui_component::Size;

    #[test]
    fn tab_title_key_distinguishes_database_and_table_lists() {
        assert_eq!(
            DatabaseObjects::tab_title_key(DbNodeType::Database),
            "Database.database"
        );
        assert_eq!(
            DatabaseObjects::tab_title_key(DbNodeType::Table),
            "DbTree.Tables"
        );
    }

    #[test]
    fn compose_tab_title_appends_database_name_for_table_like_views() {
        let table_title = DatabaseObjects::tab_title_base(DbNodeType::Table);
        let view_title = DatabaseObjects::tab_title_base(DbNodeType::View);
        let query_list_title = DatabaseObjects::tab_title_base(DbNodeType::NamedQuery);

        assert_eq!(
            DatabaseObjects::compose_tab_title(DbNodeType::Table, &table_title, Some("analytics")),
            format!("{table_title}@analytics")
        );
        assert_eq!(
            DatabaseObjects::compose_tab_title(DbNodeType::View, &view_title, Some("analytics")),
            format!("{view_title}@analytics")
        );
        assert_eq!(
            DatabaseObjects::compose_tab_title(
                DbNodeType::NamedQuery,
                &query_list_title,
                Some("analytics")
            ),
            format!("{query_list_title}@analytics")
        );
    }

    #[test]
    fn compose_tab_title_keeps_generic_titles_for_other_node_types() {
        let connection_list_title = DatabaseObjects::tab_title_base(DbNodeType::Connection);
        let database_title = DatabaseObjects::tab_title_base(DbNodeType::Database);

        assert_eq!(
            DatabaseObjects::compose_tab_title(
                DbNodeType::Connection,
                &connection_list_title,
                Some("demo")
            ),
            connection_list_title
        );
        assert_eq!(
            DatabaseObjects::compose_tab_title(DbNodeType::Database, &database_title, Some("demo")),
            database_title
        );
    }

    #[test]
    fn database_scoped_titles_use_medium_tab_width() {
        assert_eq!(
            DatabaseObjects::tab_width_size_for(DbNodeType::Table),
            Size::Medium
        );
        assert_eq!(
            DatabaseObjects::tab_width_size_for(DbNodeType::View),
            Size::Medium
        );
        assert_eq!(
            DatabaseObjects::tab_width_size_for(DbNodeType::NamedQuery),
            Size::Medium
        );
        assert_eq!(
            DatabaseObjects::tab_width_size_for(DbNodeType::Database),
            Size::XSmall
        );
    }
}
