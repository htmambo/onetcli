//! 数据传输向导第一步：源/目标端点（连接 + 数据库）选择

use gpui::{
    App, AppContext, AsyncApp, Context, Entity, IntoElement, ParentElement, SharedString, Styled,
    Window, div, prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme, IndexPath, h_flex,
    select::{SearchableVec, Select, SelectItem, SelectState},
    v_flex,
};
use rust_i18n::t;

use db::GlobalDbState;
use one_core::storage::DbConnectionConfig;

use super::view::DataTransferWindow;

const UNSELECTED_PLACEHOLDER: &str = "--";

/// 连接下拉项：value 为连接 id
#[derive(Clone, Debug)]
pub(crate) struct ConnectionItem {
    pub id: String,
    pub name: String,
}

impl SelectItem for ConnectionItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

/// 一侧端点（源或目标）的选择状态
pub(crate) struct EndpointState {
    pub connection_select: Entity<SelectState<Vec<ConnectionItem>>>,
    pub database_select: Entity<SelectState<SearchableVec<String>>>,
    pub configs: Vec<DbConnectionConfig>,
    pub loading: bool,
    pub error: Option<String>,
    /// 打开弹窗时待预选的数据库（仅源侧使用）
    pub pending_db: Option<String>,
}

impl EndpointState {
    pub fn new(
        configs: &[DbConnectionConfig],
        selected: Option<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let items: Vec<ConnectionItem> = configs
            .iter()
            .map(|c| ConnectionItem {
                id: c.id.clone(),
                name: c.name.clone(),
            })
            .collect();
        Self {
            connection_select: cx
                .new(|cx| SelectState::new(items, selected.map(IndexPath::new), window, cx)),
            database_select: cx.new(|cx| {
                SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(true)
            }),
            configs: configs.to_vec(),
            loading: false,
            error: None,
            pending_db: None,
        }
    }

    pub fn selected_config<'a>(&'a self, cx: &'a App) -> Option<&'a DbConnectionConfig> {
        let id = self.connection_select.read(cx).selected_value()?;
        self.configs.iter().find(|c| &c.id == id)
    }

    pub fn selected_database(&self, cx: &App) -> Option<String> {
        self.database_select.read(cx).selected_value().cloned()
    }

    fn render_info_panel(&self, cx: &App) -> impl IntoElement {
        let (db_type, name, host, port) = match self.selected_config(cx) {
            Some(c) => (
                c.database_type.as_str().to_string(),
                c.name.clone(),
                c.host.clone(),
                c.port.to_string(),
            ),
            None => (
                UNSELECTED_PLACEHOLDER.to_string(),
                UNSELECTED_PLACEHOLDER.to_string(),
                UNSELECTED_PLACEHOLDER.to_string(),
                UNSELECTED_PLACEHOLDER.to_string(),
            ),
        };
        v_flex()
            .gap_1()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(info_row(
                t!("DataTransfer.info_type").to_string(),
                db_type,
                cx,
            ))
            .child(info_row(t!("DataTransfer.info_name").to_string(), name, cx))
            .child(info_row(t!("DataTransfer.info_host").to_string(), host, cx))
            .child(info_row(t!("DataTransfer.info_port").to_string(), port, cx))
    }

    /// 渲染一侧端点的选择列
    pub fn render(&self, title: String, cx: &App) -> impl IntoElement {
        v_flex()
            .flex_1()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(field_label(t!("DataTransfer.connection").to_string(), cx))
            .child(
                Select::new(&self.connection_select)
                    .w_full()
                    .placeholder(t!("DataTransfer.select_connection")),
            )
            .child(field_label(t!("DataTransfer.database").to_string(), cx))
            .child(
                Select::new(&self.database_select)
                    .w_full()
                    .placeholder(t!("DataTransfer.select_database")),
            )
            .when(self.loading, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("DataTransfer.loading_databases").to_string()),
                )
            })
            .when_some(self.error.clone(), |this, err| {
                this.child(div().text_xs().text_color(cx.theme().danger).child(err))
            })
            .child(self.render_info_panel(cx))
    }
}

fn field_label(text: String, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn info_row(label: String, value: String, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .child(
            div()
                .w_20()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_xs()
                .text_ellipsis()
                .overflow_hidden()
                .child(value),
        )
}

/// 把异步加载到的数据库列表写回下拉框（需要 Window，走 active_window 惯例）
fn apply_database_items(
    select: &Entity<SelectState<SearchableVec<String>>>,
    databases: Option<Vec<String>>,
    preferred: Option<String>,
    cx: &mut AsyncApp,
) {
    let _ = cx.update(|cx: &mut App| {
        let Some(window_id) = cx.active_window() else {
            return;
        };
        let _ = cx.update_window(window_id, |_, window, cx| {
            select.update(cx, |state, cx| {
                let list = databases.unwrap_or_default();
                state.set_items(SearchableVec::new(list.clone()), window, cx);
                let index = preferred.and_then(|db| list.iter().position(|v| *v == db));
                state.set_selected_index(index.map(IndexPath::new), window, cx);
            });
        });
    });
}

impl DataTransferWindow {
    fn clear_databases(endpoint: &mut EndpointState, window: &mut Window, cx: &mut Context<Self>) {
        endpoint.loading = false;
        endpoint.error = None;
        endpoint.database_select.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(vec![]), window, cx);
            state.set_selected_index(None, window, cx);
        });
    }

    /// 连接下拉变更：注册连接并异步加载数据库列表
    pub(crate) fn on_connection_changed(
        &mut self,
        is_source: bool,
        connection_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let endpoint = if is_source {
            &mut self.source
        } else {
            &mut self.target
        };
        let Some(id) = connection_id.filter(|v| !v.is_empty()) else {
            Self::clear_databases(endpoint, window, cx);
            cx.notify();
            return;
        };
        let Some(config) = endpoint.configs.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let preferred_db = endpoint.pending_db.take();
        endpoint.loading = true;
        endpoint.error = None;
        let database_select = endpoint.database_select.clone();
        cx.notify();

        let mut global_state = cx.global::<GlobalDbState>().clone();
        global_state.register_connection(config);

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = global_state.list_databases(cx, id).await;
            let error = result
                .as_ref()
                .err()
                .map(|e| t!("DataTransfer.load_databases_failed", error = e).to_string());
            apply_database_items(&database_select, result.ok(), preferred_db, cx);
            let _ = this.update(cx, |view, cx| {
                let endpoint = if is_source {
                    &mut view.source
                } else {
                    &mut view.target
                };
                endpoint.loading = false;
                endpoint.error = error;
                cx.notify();
            });
        })
        .detach();
    }

    /// 渲染第一步：源/目标两栏 + 同库警告
    pub(crate) fn render_endpoints_step(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_3()
            .p_4()
            .child(
                h_flex()
                    .flex_1()
                    .gap_4()
                    .child(
                        self.source
                            .render(t!("DataTransfer.source").to_string(), cx),
                    )
                    .child(div().w_px().bg(cx.theme().border))
                    .child(
                        self.target
                            .render(t!("DataTransfer.target").to_string(), cx),
                    ),
            )
            .when(self.same_endpoint(cx), |this| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .text_sm()
                        .bg(cx.theme().warning.opacity(0.15))
                        .text_color(cx.theme().warning)
                        .child(t!("DataTransfer.same_endpoint_warning").to_string()),
                )
            })
    }
}
