//! 数据传输向导第二步：源库对象（表/视图）多选，带分组全选与搜索过滤

use std::collections::HashSet;

use gpui::{
    App, AppContext, AsyncApp, Context, Entity, IntoElement, ParentElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme,
    checkbox::Checkbox,
    input::{Input, InputState},
    scroll::ScrollableElement,
    v_flex,
};
use rust_i18n::t;

use db::GlobalDbState;

use super::view::DataTransferWindow;

/// 对象选择状态：表/视图清单、选中集合与搜索词
pub(crate) struct ObjectsState {
    pub tables: Vec<String>,
    pub views: Vec<String>,
    pub selected_tables: HashSet<String>,
    pub selected_views: HashSet<String>,
    pub search: Entity<InputState>,
    pub query: String,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
}

impl ObjectsState {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            tables: vec![],
            views: vec![],
            selected_tables: HashSet::new(),
            selected_views: HashSet::new(),
            search: cx.new(|cx| {
                InputState::new(window, cx).placeholder(t!("DataTransfer.search_placeholder"))
            }),
            query: String::new(),
            loading: false,
            loaded: false,
            error: None,
        }
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_tables.is_empty() || !self.selected_views.is_empty()
    }

    fn filtered(items: &[String], query: &str) -> Vec<String> {
        let needle = query.to_lowercase();
        items
            .iter()
            .filter(|n| needle.is_empty() || n.to_lowercase().contains(&needle))
            .cloned()
            .collect()
    }

    pub fn filtered_tables(&self) -> Vec<String> {
        Self::filtered(&self.tables, &self.query)
    }

    pub fn filtered_views(&self) -> Vec<String> {
        Self::filtered(&self.views, &self.query)
    }

    fn selected_in_order(items: &[String], selected: &HashSet<String>) -> Vec<String> {
        items
            .iter()
            .filter(|n| selected.contains(*n))
            .cloned()
            .collect()
    }

    pub fn selected_tables_in_order(&self) -> Vec<String> {
        Self::selected_in_order(&self.tables, &self.selected_tables)
    }

    pub fn selected_views_in_order(&self) -> Vec<String> {
        Self::selected_in_order(&self.views, &self.selected_views)
    }

    fn toggle(selected: &mut HashSet<String>, name: &str) {
        if !selected.remove(name) {
            selected.insert(name.to_string());
        }
    }

    pub fn toggle_table(&mut self, name: &str) {
        Self::toggle(&mut self.selected_tables, name);
    }

    pub fn toggle_view(&mut self, name: &str) {
        Self::toggle(&mut self.selected_views, name);
    }

    fn set_group(selected: &mut HashSet<String>, names: &[String], check: bool) {
        for name in names {
            if check {
                selected.insert(name.clone());
            } else {
                selected.remove(name);
            }
        }
    }

    pub fn set_table_group(&mut self, names: &[String], check: bool) {
        Self::set_group(&mut self.selected_tables, names, check);
    }

    pub fn set_view_group(&mut self, names: &[String], check: bool) {
        Self::set_group(&mut self.selected_views, names, check);
    }

    /// 首次加载后默认全选
    fn set_objects(&mut self, tables: Vec<String>, views: Vec<String>) {
        self.selected_tables = tables.iter().cloned().collect();
        self.selected_views = views.iter().cloned().collect();
        self.tables = tables;
        self.views = views;
    }
}

impl DataTransferWindow {
    /// 进入第二步时加载源库的表与视图清单
    pub(crate) fn load_objects(&mut self, cx: &mut Context<Self>) {
        if self.objects.loaded || self.objects.loading {
            return;
        }
        let Some(config) = self.source.selected_config(cx).cloned() else {
            return;
        };
        let Some(database) = self.source.selected_database(cx) else {
            return;
        };
        self.objects.loading = true;
        self.objects.error = None;
        cx.notify();

        let global_state = cx.global::<GlobalDbState>().clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let conn_id = config.id.clone();
            let tables = global_state
                .list_tables(cx, conn_id.clone(), database.clone(), None)
                .await
                .map(|list| list.into_iter().map(|t| t.name).collect::<Vec<_>>());
            let views = global_state
                .list_views_view(cx, conn_id, database)
                .await
                .map(|view| {
                    view.rows
                        .into_iter()
                        .filter_map(|row| row.into_iter().next())
                        .collect::<Vec<_>>()
                });
            let _ = this.update(cx, |view, cx| {
                view.objects.loading = false;
                view.objects.loaded = true;
                match (tables, views) {
                    (Ok(t), Ok(v)) => view.objects.set_objects(t, v),
                    (Err(e), _) | (_, Err(e)) => {
                        view.objects.error =
                            Some(t!("DataTransfer.objects_load_failed", error = e).to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn render_object_item(
    checked: bool,
    is_table: bool,
    index: usize,
    name: &str,
    cx: &mut Context<DataTransferWindow>,
) -> impl IntoElement + use<> {
    let name = name.to_string();
    let name_for_toggle = name.clone();
    let item_id = if is_table { "table-item" } else { "view-item" };
    div().pl_6().child(
        Checkbox::new((item_id, index))
            .checked(checked)
            .label(name)
            .on_click(cx.listener(move |view, _, _window, cx| {
                if is_table {
                    view.objects.toggle_table(&name_for_toggle);
                } else {
                    view.objects.toggle_view(&name_for_toggle);
                }
                cx.notify();
            })),
    )
}

impl DataTransferWindow {
    fn render_object_group(
        &self,
        is_table: bool,
        names: Vec<String>,
        total: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = if is_table {
            &self.objects.selected_tables
        } else {
            &self.objects.selected_views
        };
        let title = if is_table {
            t!("DataTransfer.tables_group", count = total).to_string()
        } else {
            t!("DataTransfer.views_group", count = total).to_string()
        };
        let all_checked = !names.is_empty() && names.iter().all(|n| selected.contains(n));
        let names_for_toggle = names.clone();

        v_flex()
            .gap_1()
            .child(
                Checkbox::new(if is_table {
                    "group-tables"
                } else {
                    "group-views"
                })
                .checked(all_checked)
                .label(title)
                .on_click(cx.listener(move |view, checked, _window, cx| {
                    if is_table {
                        view.objects.set_table_group(&names_for_toggle, *checked);
                    } else {
                        view.objects.set_view_group(&names_for_toggle, *checked);
                    }
                    cx.notify();
                })),
            )
            .children(names.iter().enumerate().map(|(index, name)| {
                render_object_item(selected.contains(name), is_table, index, name, cx)
            }))
    }

    /// 渲染第二步：分组对象多选 + 底部搜索框
    pub(crate) fn render_objects_step(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let filtered_tables = self.objects.filtered_tables();
        let filtered_views = self.objects.filtered_views();
        let total_tables = self.objects.tables.len();
        let total_views = self.objects.views.len();

        v_flex()
            .size_full()
            .gap_2()
            .p_4()
            .when(self.objects.loading, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("DataTransfer.objects_loading").to_string()),
                )
            })
            .when_some(self.objects.error.clone(), |this, err| {
                this.child(div().text_sm().text_color(cx.theme().danger).child(err))
            })
            .when(
                !self.objects.loading && self.objects.error.is_none(),
                |this| {
                    this.child(
                        // Scrollable 包装器只继承 size，flex_1/min_h_0 须加在外层普通容器上
                        div().flex_1().min_h_0().child(
                            div().size_full().overflow_y_scrollbar().child(
                                v_flex()
                                    .gap_3()
                                    .child(self.render_object_group(
                                        true,
                                        filtered_tables,
                                        total_tables,
                                        cx,
                                    ))
                                    .child(self.render_object_group(
                                        false,
                                        filtered_views,
                                        total_views,
                                        cx,
                                    )),
                            ),
                        ),
                    )
                },
            )
            .child(Input::new(&self.objects.search).w_full())
    }
}
