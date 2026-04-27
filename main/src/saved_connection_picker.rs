use std::cmp::Ordering;
use std::collections::HashSet;

use crate::onetcli_app::GlobalHomePage;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Size, StyledExt,
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    popover::Popover,
    tokens::Radius,
    v_flex,
};
use one_core::storage::{ActiveConnections, ConnectionType, StoredConnection, Workspace};
use rust_i18n::t;
use one_core::tab_container::WINDOW_CONTROL_BUTTON_SIZE;

#[derive(Clone)]
pub(crate) enum SavedConnectionPickerEvent {
    ConnectionSelected(StoredConnection),
    NewTerminalRequested,
}

#[derive(Clone)]
struct SavedConnectionPickerSection {
    workspace_id: Option<i64>,
    title: String,
    connections: Vec<StoredConnection>,
}

#[derive(Clone)]
struct VisibleSavedConnectionSection {
    workspace_id: Option<i64>,
    title: String,
    connections: Vec<StoredConnection>,
    expanded: bool,
}

pub(crate) struct SavedConnectionPickerList {
    search_input: Entity<InputState>,
    search_query: String,
    sections: Vec<SavedConnectionPickerSection>,
    collapsed_workspace_ids: HashSet<i64>,
    unassigned_collapsed: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SavedConnectionPickerEvent> for SavedConnectionPickerList {}

impl SavedConnectionPickerList {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("Home.saved_connection_picker_search_placeholder"))
                .clean_on_escape()
        });

        let mut this = Self {
            search_input: search_input.clone(),
            search_query: String::new(),
            sections: Vec::new(),
            collapsed_workspace_ids: HashSet::new(),
            unassigned_collapsed: false,
            _subscriptions: Vec::new(),
        };

        let subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _window, cx| match event {
                InputEvent::Change => {
                    this.search_query = input.read(cx).text().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.open_first_visible_connection(cx);
                }
                InputEvent::Focus | InputEvent::Blur => {}
            },
        );
        this._subscriptions.push(subscription);
        this
    }

    pub(crate) fn refresh(
        &mut self,
        workspaces: &[Workspace],
        connections: &[StoredConnection],
        cx: &mut Context<Self>,
    ) {
        self.sections = build_sections(workspaces, connections);
        let valid_workspace_ids: HashSet<i64> = self
            .sections
            .iter()
            .filter_map(|section| section.workspace_id)
            .collect();
        self.collapsed_workspace_ids
            .retain(|workspace_id| valid_workspace_ids.contains(workspace_id));
        if !self
            .sections
            .iter()
            .any(|section| section.workspace_id.is_none())
        {
            self.unassigned_collapsed = false;
        }

        cx.notify();
    }

    pub(crate) fn refresh_from_home(&mut self, cx: &mut Context<Self>) {
        let Some(home) = cx.try_global::<GlobalHomePage>() else {
            self.sections.clear();
            cx.notify();
            return;
        };

        let (workspaces, connections) = {
            let home = home.home_page.read(cx);
            (home.workspaces.clone(), home.connections.clone())
        };

        self.refresh(&workspaces, &connections, cx);
    }

    pub(crate) fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    fn toggle_section(&mut self, workspace_id: Option<i64>, cx: &mut Context<Self>) {
        match workspace_id {
            Some(workspace_id) => {
                if self.collapsed_workspace_ids.contains(&workspace_id) {
                    self.collapsed_workspace_ids.remove(&workspace_id);
                } else {
                    self.collapsed_workspace_ids.insert(workspace_id);
                }
            }
            None => {
                self.unassigned_collapsed = !self.unassigned_collapsed;
            }
        }
        cx.notify();
    }

    fn visible_sections(&self) -> Vec<VisibleSavedConnectionSection> {
        let query = self.search_query.trim().to_lowercase();
        let is_searching = !query.is_empty();

        self.sections
            .iter()
            .filter_map(|section| {
                let workspace_match = is_searching && section.title.to_lowercase().contains(&query);
                let connections: Vec<StoredConnection> = section
                    .connections
                    .iter()
                    .filter(|connection| {
                        !is_searching
                            || workspace_match
                            || connection_matches_query(connection, &query)
                    })
                    .cloned()
                    .collect();

                if connections.is_empty() {
                    return None;
                }

                let expanded = if is_searching {
                    true
                } else {
                    match section.workspace_id {
                        Some(workspace_id) => !self.collapsed_workspace_ids.contains(&workspace_id),
                        None => !self.unassigned_collapsed,
                    }
                };

                Some(VisibleSavedConnectionSection {
                    workspace_id: section.workspace_id,
                    title: section.title.clone(),
                    connections,
                    expanded,
                })
            })
            .collect()
    }

    fn select_connection(&mut self, connection: StoredConnection, cx: &mut Context<Self>) {
        cx.emit(SavedConnectionPickerEvent::ConnectionSelected(connection));
    }

    fn emit_new_terminal(&mut self, cx: &mut Context<Self>) {
        cx.emit(SavedConnectionPickerEvent::NewTerminalRequested);
    }

    fn open_first_visible_connection(&mut self, cx: &mut Context<Self>) {
        let Some(connection) = self
            .visible_sections()
            .into_iter()
            .flat_map(|section| section.connections.into_iter())
            .next()
        else {
            return;
        };

        self.select_connection(connection, cx);
    }

    fn render_connection_status_indicator(cx: &App) -> AnyElement {
        div()
            .absolute()
            .top(px(4.0))
            .left(px(15.0))
            .w(px(12.0))
            .h(px(12.0))
            .rounded_full()
            .bg(cx.theme().success)
            .shadow_lg()
            .flex() // 新增：让内部内容居中
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check) // 白色钩号
                    .with_size(px(11.0))
                    .text_color(gpui::white()),
            )
            .into_any_element()
    }

    fn render_section(
        &self,
        section: VisibleSavedConnectionSection,
        owner: Entity<Self>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workspace_id = section.workspace_id;
        let is_expanded = section.expanded;
        let section_title = section.title.clone();
        let visible_count = section.connections.len();
        let header_owner = owner.clone();

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .id(SharedString::from(format!(
                        "saved-connection-picker-section-{}",
                        workspace_id.unwrap_or(0)
                    )))
                    .w_full()
                    .items_center()
                    // .gap_1()
                    .px_1()
                    .py_1()
                    .rounded(Radius::Md.px())
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().list_hover))
                    .on_click(move |_, _, cx| {
                        header_owner.update(cx, |this, cx| {
                            this.toggle_section(workspace_id, cx);
                        });
                    })
                    .child(
                        Icon::new(if is_expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .with_size(Size::Small)
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        Icon::new(if workspace_id.is_some() {
                            IconName::AppsColor
                        } else {
                            IconName::Folder
                        })
                        .with_size(Size::Small)
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_semibold()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(section_title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{visible_count}")),
                    ),
            )
            .when(is_expanded, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .children(section.connections.into_iter().map(|connection| {
                            let owner = owner.clone();
                            let connection_for_open = connection.clone();
                            let connection_name = connection.name.clone();
                            let connection_type = connection.connection_type;
                            let is_active = connection.id.map_or(false, |id| {
                                cx.try_global::<ActiveConnections>()
                                    .map(|active| active.is_active(id))
                                    .unwrap_or(false)
                            });

                            div()
                                .id(SharedString::from(format!(
                                    "saved-connection-picker-item-{}",
                                    connection.id.unwrap_or(0)
                                )))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_1()
                                .pl_5()
                                // .px_1()
                                // .py_1p5()
                                .relative()
                                .rounded(Radius::Md.px())
                                .cursor_pointer()
                                .hover(|style| style.bg(cx.theme().list_hover))
                                .on_click(move |_, _, cx| {
                                    owner.update(cx, |this, cx| {
                                        this.select_connection(connection_for_open.clone(), cx);
                                    });
                                })
                                .child(
                                    Icon::new(connection_type.icon())
                                        .color()
                                        .with_size(Size::Large),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .text_sm()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .child(connection_name),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(connection_type.label()),
                                        ),
                                )
                                .when(is_active, |this| {
                                    this.child(Self::render_connection_status_indicator(cx))
                                })
                        })),
                )
            })
    }
}

impl Focusable for SavedConnectionPickerList {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.read(cx).focus_handle(cx)
    }
}

impl Render for SavedConnectionPickerList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity();
        let visible_sections = self.visible_sections();
        let list_content: AnyElement = if visible_sections.is_empty() {
            v_flex()
                .w_full()
                .items_center()
                .justify_center()
                .gap_2()
                .px_3()
                .py_6()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Inbox).size_8())
                .child(
                    div()
                        .text_sm()
                        .child(t!("Home.saved_connection_picker_empty")),
                )
                .into_any_element()
        } else {
            let mut sections_container = v_flex().w_full().gap_2();
            for section in visible_sections {
                sections_container = sections_container.child(self.render_section(
                    section,
                    owner.clone(),
                    window,
                    cx,
                ));
            }
            sections_container.into_any_element()
        };

        v_flex()
            .id("saved-connection-picker-list")
            .w_full()
            .min_h_0()
            .gap_2()
            .child(Input::new(&self.search_input).small().w_full())
            .child(
                div()
                    .id("new-terminal-btn")
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .rounded(Radius::Md.px())
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .hover(|style| style.bg(cx.theme().list_hover))
                    .on_click(window.listener_for(&cx.entity(), |this, _, _, cx| {
                        this.emit_new_terminal(cx);
                    }))
                    .child(
                        Icon::new(IconName::Terminal)
                            .size_5()
                            .text_color(gpui::rgb(0x8b5cf6)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0x8b5cf6))
                            .child(t!("Home.terminal")),
                    ),
            )
            .child(
                div()
                    .id("saved-connection-picker-scroll")
                    .w_full()
                    .max_h(px(360.0))
                    .overflow_y_scroll()
                    .child(list_content),
            )
    }
}

pub(crate) struct TabBarSavedConnectionPicker {
    picker: Entity<SavedConnectionPickerList>,
    popover_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl TabBarSavedConnectionPicker {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| SavedConnectionPickerList::new(window, cx));

        let subscription = cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &SavedConnectionPickerEvent, window, cx| {
                match event {
                    SavedConnectionPickerEvent::ConnectionSelected(connection) => {
                        if let Some(home_page) = cx
                            .try_global::<GlobalHomePage>()
                            .map(|global| global.home_page.clone())
                        {
                            home_page.update(cx, |home, cx| {
                                home.open_connection_from_saved_picker(connection, window, cx);
                            });
                        }
                    }
                    SavedConnectionPickerEvent::NewTerminalRequested => {
                        if let Some(home_page) = cx
                            .try_global::<GlobalHomePage>()
                            .map(|global| global.home_page.clone())
                        {
                            home_page.update(cx, |home, cx| {
                                home.add_terminal_tab(window, cx);
                            });
                        }
                    }
                }
                this.popover_open = false;
                cx.notify();
            },
        );

        Self {
            picker,
            popover_open: false,
            _subscriptions: vec![subscription],
        }
    }

    fn set_popover_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.popover_open = open;
        if open {
            self.picker.update(cx, |picker, cx| {
                picker.refresh_from_home(cx);
                picker.focus_search(window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_popover_open(true, window, cx);
    }
}

impl Render for TabBarSavedConnectionPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = self.picker.clone();
        let theme = cx.theme();
        let active_tab_color = theme.tab_active;
        let hover_tab_color = theme.tab.opacity(0.8);
        let inactive_tab_color = theme.tab.opacity(0.5);
        let text_color = theme.tab_foreground;

        Popover::new("tab-bar-saved-connection-picker")
            .trigger(
                Button::new("tab-bar-saved-connection-picker-trigger")
                    .icon(IconName::Plus)
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(inactive_tab_color)
                            .foreground(text_color)
                            .hover(hover_tab_color)
                            .active(active_tab_color),
                    )
                    .compact()
                    .h(WINDOW_CONTROL_BUTTON_SIZE)
                    .w(WINDOW_CONTROL_BUTTON_SIZE)
                    .rounded(Radius::Md.px())
                    .cursor_pointer()
                    .tooltip(t!("Home.saved_connection_picker_trigger")),
            )
            .open(self.popover_open)
            .on_open_change(cx.listener(move |this, open, window, cx| {
                this.set_popover_open(*open, window, cx);
            }))
            .content(move |_, _, cx| {
                div()
                    .w(px(340.0))
                    .max_h(px(420.0))
                    .p_2()
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .overflow_hidden()
                    .shadow_lg()
                    .child(picker.clone())
            })
    }
}

fn build_sections(
    workspaces: &[Workspace],
    connections: &[StoredConnection],
) -> Vec<SavedConnectionPickerSection> {
    let mut sections = Vec::new();

    let mut ordered_workspaces: Vec<Workspace> = workspaces
        .iter()
        .filter(|workspace| workspace.id.is_some())
        .cloned()
        .collect();
    ordered_workspaces.sort_by(compare_workspaces_manual);

    for workspace in ordered_workspaces {
        let workspace_id = workspace.id;
        let mut workspace_connections: Vec<StoredConnection> = connections
            .iter()
            .filter(|connection| {
                connection.workspace_id == workspace_id
                    && supports_saved_connection_picker(connection.connection_type)
            })
            .cloned()
            .collect();
        workspace_connections.sort_by(compare_connections_manual);

        if workspace_connections.is_empty() {
            continue;
        }

        sections.push(SavedConnectionPickerSection {
            workspace_id,
            title: workspace.name,
            connections: workspace_connections,
        });
    }

    let mut unassigned_connections: Vec<StoredConnection> = connections
        .iter()
        .filter(|connection| {
            connection.workspace_id.is_none()
                && supports_saved_connection_picker(connection.connection_type)
        })
        .cloned()
        .collect();
    unassigned_connections.sort_by(compare_connections_manual);
    if !unassigned_connections.is_empty() {
        sections.push(SavedConnectionPickerSection {
            workspace_id: None,
            title: t!("Home.unassigned_workspace").to_string(),
            connections: unassigned_connections,
        });
    }

    sections
}

fn supports_saved_connection_picker(connection_type: ConnectionType) -> bool {
    matches!(
        connection_type,
        ConnectionType::Database
            | ConnectionType::SshSftp
            | ConnectionType::Redis
            | ConnectionType::MongoDB
            | ConnectionType::Serial
    )
}

fn connection_matches_query(connection: &StoredConnection, query: &str) -> bool {
    connection.name.to_lowercase().contains(query)
        || connection
            .connection_type
            .label()
            .to_lowercase()
            .contains(query)
        || connection
            .id
            .map(|id| id.to_string().contains(query))
            .unwrap_or(false)
}

fn compare_workspaces_manual(a: &Workspace, b: &Workspace) -> Ordering {
    manual_sort_value(a.sort_order)
        .cmp(&manual_sort_value(b.sort_order))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
        .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
        .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0)))
}

fn compare_connections_manual(a: &StoredConnection, b: &StoredConnection) -> Ordering {
    manual_sort_value(a.sort_order)
        .cmp(&manual_sort_value(b.sort_order))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
        .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
        .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0)))
}

fn timestamp_value(value: Option<i64>) -> i64 {
    value.unwrap_or(0)
}

fn manual_sort_value(value: Option<i64>) -> i64 {
    value.unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{build_sections, connection_matches_query, supports_saved_connection_picker};
    use one_core::storage::{ConnectionType, StoredConnection, Workspace};

    fn workspace(id: i64, name: &str, sort_order: i64) -> Workspace {
        let mut workspace = Workspace::new(name.to_string());
        workspace.id = Some(id);
        workspace.sort_order = Some(sort_order);
        workspace
    }

    fn connection(
        id: i64,
        name: &str,
        connection_type: ConnectionType,
        workspace_id: Option<i64>,
        sort_order: i64,
    ) -> StoredConnection {
        StoredConnection {
            id: Some(id),
            name: name.to_string(),
            connection_type,
            params: "{}".to_string(),
            sort_order: Some(sort_order),
            workspace_id,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    #[test]
    fn build_sections_按手动顺序分组并把未分配放到最后() {
        let workspaces = vec![workspace(1, "后排工作区", 2), workspace(2, "前排工作区", 0)];
        let connections = vec![
            connection(11, "B-数据库", ConnectionType::Database, Some(2), 5),
            connection(12, "A-Redis", ConnectionType::Redis, Some(2), 1),
            connection(21, "串口", ConnectionType::Serial, Some(1), 0),
            connection(31, "未分配 SSH", ConnectionType::SshSftp, None, 0),
        ];

        let sections = build_sections(&workspaces, &connections);

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].title, "前排工作区");
        assert_eq!(sections[0].connections[0].name, "A-Redis");
        assert_eq!(sections[0].connections[1].name, "B-数据库");
        assert_eq!(sections[1].title, "后排工作区");
        assert_eq!(sections[2].workspace_id, None);
        assert_eq!(sections[2].connections[0].name, "未分配 SSH");
    }

    #[test]
    fn supports_saved_connection_picker_排除_chatdb() {
        assert!(supports_saved_connection_picker(ConnectionType::Database));
        assert!(!supports_saved_connection_picker(ConnectionType::ChatDB));
    }

    #[test]
    fn connection_matches_query_支持名称类型和_id() {
        let connection = connection(42, "生产 Redis", ConnectionType::Redis, None, 0);

        assert!(connection_matches_query(&connection, "redis"));
        assert!(connection_matches_query(&connection, "生产"));
        assert!(connection_matches_query(&connection, "42"));
        assert!(!connection_matches_query(&connection, "mongo"));
    }
}
