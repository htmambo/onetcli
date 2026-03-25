//! MongoDB 连接表单窗口

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
    div, px,
};
use gpui_component::{
    Disableable, IconName, Sizable, Size, StyledExt, TitleBar, app_style,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectDelegate, SelectEvent, SelectItem, SelectState},
    tab::{Tab, TabBar},
    v_flex,
};
use one_core::certificate_manager::open_certificate_manager_popup;
use one_core::certificate_notifier::{
    CertificateDataEvent, get_notifier as get_certificate_notifier,
};
use one_core::cloud_sync::GlobalCloudUser;
use one_core::connection_notifier::{ConnectionDataEvent, get_notifier};
use one_core::gpui_tokio::Tokio;
use one_core::storage::traits::Repository;
use one_core::storage::{
    Certificate, CertificateKind, CertificateReference, CertificateRepository, MongoDBParams,
    StoredConnection, Workspace,
};
use rust_i18n::t;
use tracing::error;

use crate::MongoManager;

/// MongoDB 表单窗口配置
pub struct MongoFormWindowConfig {
    pub editing_connection: Option<StoredConnection>,
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Default, PartialEq)]
struct WorkspaceSelectItem {
    id: Option<i64>,
    name: String,
}

impl WorkspaceSelectItem {
    fn none() -> Self {
        Self {
            id: None,
            name: t!("Common.none").to_string(),
        }
    }

    fn from_workspace(workspace: &Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name.clone(),
        }
    }
}

impl SelectItem for WorkspaceSelectItem {
    type Value = Option<i64>;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

#[derive(Clone, Default, PartialEq)]
struct CertificateSelectItem {
    id: Option<i64>,
    label: String,
}

impl CertificateSelectItem {
    fn none() -> Self {
        Self {
            id: None,
            label: t!("MongoForm.certificate_none").to_string(),
        }
    }

    fn from_certificate(certificate: &Certificate) -> Self {
        Self {
            id: certificate.id,
            label: format!("{} · {}", certificate.name, certificate.kind.label()),
        }
    }
}

impl SelectItem for CertificateSelectItem {
    type Value = Option<i64>;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

/// MongoDB 连接表单窗口
pub struct MongoFormWindow {
    focus_handle: FocusHandle,
    title: SharedString,
    is_editing: bool,
    editing_id: Option<i64>,
    editing_cloud_id: Option<String>,
    editing_last_synced_at: Option<i64>,

    active_tab: usize,

    name_input: Entity<InputState>,
    host_input: Entity<InputState>,
    port_input: Entity<InputState>,
    database_input: Entity<InputState>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    certificates: Vec<Certificate>,
    credential_select: Entity<SelectState<Vec<CertificateSelectItem>>>,
    authentication_source_input: Entity<InputState>,
    replica_set_input: Entity<InputState>,
    read_preference_input: Entity<InputState>,
    connect_timeout_seconds_input: Entity<InputState>,
    application_name_input: Entity<InputState>,

    use_srv_record: bool,
    direct_connection: bool,
    use_tls: bool,

    workspace_select: Entity<SelectState<Vec<WorkspaceSelectItem>>>,
    remark_input: Entity<InputState>,
    sync_enabled: bool,

    is_testing: bool,
    test_result: Option<Result<(), String>>,
    _subscriptions: Vec<Subscription>,
}

impl MongoFormWindow {
    pub fn new(config: MongoFormWindowConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let is_editing = config.editing_connection.is_some();
        let editing_id = config.editing_connection.as_ref().and_then(|c| c.id);
        let editing_cloud_id = config
            .editing_connection
            .as_ref()
            .and_then(|c| c.cloud_id.clone());
        let editing_last_synced_at = config
            .editing_connection
            .as_ref()
            .and_then(|c| c.last_synced_at);

        let title: SharedString = if is_editing {
            t!("MongoForm.edit_connection_title").to_string()
        } else {
            t!("MongoForm.new_connection_title").to_string()
        }
        .into();

        let existing_parameters = config
            .editing_connection
            .as_ref()
            .and_then(|connection| connection.to_mongodb_params().ok());
        let certificates = cx
            .global::<one_core::storage::GlobalStorageState>()
            .storage
            .get::<CertificateRepository>()
            .and_then(|repo| repo.list().ok())
            .unwrap_or_default();
        let mut certificate_items = vec![CertificateSelectItem::none()];
        certificate_items.extend(
            certificates
                .iter()
                .filter(|certificate| certificate.kind == CertificateKind::UsernamePassword)
                .map(CertificateSelectItem::from_certificate),
        );
        let credential_select =
            cx.new(|cx| SelectState::new(certificate_items, Some(Default::default()), window, cx));

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.name_placeholder").to_string());
            if let Some(connection) = &config.editing_connection {
                state.set_value(connection.name.clone(), window, cx);
            }
            state
        });

        let host_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.host_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if !parameters.host.is_empty() {
                    state.set_value(parameters.host.clone(), window, cx);
                }
            }
            state
        });

        let port_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.port_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(port_value) = parameters.port {
                    state.set_value(port_value.to_string(), window, cx);
                } else {
                    state.set_value("27017", window, cx);
                }
            } else {
                state.set_value("27017", window, cx);
            }
            state
        });

        let database_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.database_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(database) = &parameters.database {
                    if !database.is_empty() {
                        state.set_value(database.clone(), window, cx);
                    }
                }
            }
            state
        });

        let username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.username_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(username) = &parameters.username {
                    if !username.is_empty() {
                        state.set_value(username.clone(), window, cx);
                    }
                }
            }
            state
        });

        let password_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.password_placeholder").to_string())
                .masked(true);
            if let Some(parameters) = &existing_parameters {
                if let Some(password) = &parameters.password {
                    if !password.is_empty() {
                        state.set_value(password.clone(), window, cx);
                    }
                }
            }
            state
        });

        let authentication_source_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.auth_source_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(auth_source) = &parameters.auth_source {
                    if !auth_source.is_empty() {
                        state.set_value(auth_source.clone(), window, cx);
                    }
                }
            }
            state
        });

        let replica_set_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.replica_set_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(replica_set) = &parameters.replica_set {
                    if !replica_set.is_empty() {
                        state.set_value(replica_set.clone(), window, cx);
                    }
                }
            }
            state
        });

        let read_preference_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.read_preference_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(read_preference) = &parameters.read_preference {
                    if !read_preference.is_empty() {
                        state.set_value(read_preference.clone(), window, cx);
                    }
                }
            }
            state
        });

        let connect_timeout_seconds_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.connect_timeout_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(timeout_seconds) = parameters.connect_timeout_seconds {
                    state.set_value(timeout_seconds.to_string(), window, cx);
                }
            }
            state
        });

        let application_name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.app_name_placeholder").to_string());
            if let Some(parameters) = &existing_parameters {
                if let Some(application_name) = &parameters.application_name {
                    if !application_name.is_empty() {
                        state.set_value(application_name.clone(), window, cx);
                    }
                }
            }
            state
        });

        let remark_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("MongoForm.remark_placeholder").to_string());
            if let Some(connection) = &config.editing_connection {
                if let Some(remark) = &connection.remark {
                    state.set_value(remark.clone(), window, cx);
                }
            }
            state
        });

        let workspace_items = {
            let mut items = vec![WorkspaceSelectItem::none()];
            items.extend(
                config
                    .workspaces
                    .iter()
                    .map(WorkspaceSelectItem::from_workspace),
            );
            items
        };

        let workspace_select = cx.new(|cx| {
            let mut state = SelectState::new(workspace_items, None, window, cx);
            if let Some(selected) = config
                .editing_connection
                .as_ref()
                .and_then(|connection| connection.workspace_id)
            {
                state.set_selected_value(&Some(selected), window, cx);
            }
            state
        });

        let sync_enabled = config
            .editing_connection
            .as_ref()
            .map(|connection| connection.sync_enabled)
            .unwrap_or(true);

        let use_srv_record = existing_parameters
            .as_ref()
            .map(|parameters| parameters.use_srv_record)
            .unwrap_or(false);
        let direct_connection = existing_parameters
            .as_ref()
            .map(|parameters| parameters.direct_connection)
            .unwrap_or(false);
        let use_tls = existing_parameters
            .as_ref()
            .map(|parameters| parameters.use_tls)
            .unwrap_or(false);

        let editing_credential_ref = existing_parameters
            .as_ref()
            .and_then(|parameters| parameters.credential_ref.clone());

        let mut view = Self {
            focus_handle: cx.focus_handle(),
            title,
            is_editing,
            editing_id,
            editing_cloud_id,
            editing_last_synced_at,
            active_tab: 0,
            name_input,
            host_input,
            port_input,
            database_input,
            username_input,
            password_input,
            certificates,
            credential_select,
            authentication_source_input,
            replica_set_input,
            read_preference_input,
            connect_timeout_seconds_input,
            application_name_input,
            use_srv_record,
            direct_connection,
            use_tls,
            workspace_select,
            remark_input,
            sync_enabled,
            is_testing: false,
            test_result: None,
            _subscriptions: Vec::new(),
        };

        cx.subscribe_in(
            &view.credential_select,
            window,
            |this, _select, event: &SelectEvent<Vec<CertificateSelectItem>>, window, cx| {
                let SelectEvent::Confirm(_) = event;
                this.sync_selected_certificate_inputs(window, cx);
                cx.notify();
            },
        )
        .detach();

        if let Some(notifier) = get_certificate_notifier(cx) {
            view._subscriptions.push(cx.subscribe_in(
                &notifier,
                window,
                |this, _, _event: &CertificateDataEvent, window, cx| {
                    this.reload_certificates(window, cx);
                },
            ));
        }

        view.restore_selected_certificate(editing_credential_ref.as_ref(), window, cx);
        view.sync_selected_certificate_inputs(window, cx);
        view
    }

    fn get_workspace_id(&self, cx: &App) -> Option<i64> {
        self.workspace_select
            .read(cx)
            .selected_value()
            .cloned()
            .flatten()
    }

    fn restore_selected_certificate(
        &mut self,
        reference: Option<&CertificateReference>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = reference else {
            return;
        };

        let selected_id = self
            .certificates
            .iter()
            .find(|certificate| reference.matches_certificate(certificate))
            .and_then(|certificate| certificate.id);
        self.credential_select.update(cx, |select, cx| {
            select.set_selected_value(&selected_id, window, cx);
        });
    }

    fn selected_certificate(&self, cx: &App) -> Option<Certificate> {
        let selected_id = self
            .credential_select
            .read(cx)
            .selected_value()
            .cloned()
            .flatten();

        self.certificates
            .iter()
            .find(|certificate| certificate.id == selected_id)
            .cloned()
    }

    fn reload_certificates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_id = self
            .credential_select
            .read(cx)
            .selected_value()
            .cloned()
            .flatten();

        self.certificates = cx
            .global::<one_core::storage::GlobalStorageState>()
            .storage
            .get::<CertificateRepository>()
            .and_then(|repo| repo.list().ok())
            .unwrap_or_default();

        let mut items = vec![CertificateSelectItem::none()];
        items.extend(
            self.certificates
                .iter()
                .filter(|certificate| certificate.kind == CertificateKind::UsernamePassword)
                .map(CertificateSelectItem::from_certificate),
        );

        self.credential_select.update(cx, |select, cx| {
            select.set_items(items, window, cx);
            select.set_selected_value(&selected_id, window, cx);
        });
        self.sync_selected_certificate_inputs(window, cx);
        cx.notify();
    }

    fn sync_selected_certificate_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(certificate) = self.selected_certificate(cx) else {
            return;
        };

        self.username_input.update(cx, |state, cx| {
            state.set_value(&certificate.username, window, cx)
        });
        self.password_input.update(cx, |state, cx| {
            state.set_value(certificate.password.clone().unwrap_or_default(), window, cx);
        });
    }

    fn build_parameters(&self, cx: &App) -> Result<MongoDBParams, String> {
        let selected_certificate = self.selected_certificate(cx);
        let host_value = self.host_input.read(cx).text().to_string();
        let host_value = host_value.trim().to_string();
        if host_value.is_empty() {
            return Err(t!("MongoForm.host_required").to_string());
        }

        let port_text = self.port_input.read(cx).text().to_string();
        let port_text = port_text.trim().to_string();
        let port_value = if port_text.is_empty() {
            None
        } else {
            Some(
                port_text
                    .parse::<u16>()
                    .map_err(|_| t!("MongoForm.port_must_be_number").to_string())?,
            )
        };

        let database_value = self.database_input.read(cx).text().to_string();
        let database_value = database_value.trim().to_string();
        let database = if database_value.is_empty() {
            None
        } else {
            Some(database_value)
        };

        let username = selected_certificate
            .as_ref()
            .map(|certificate| Some(certificate.username.clone()))
            .unwrap_or_else(|| {
                let username_value = self.username_input.read(cx).text().to_string();
                let username_value = username_value.trim().to_string();
                if username_value.is_empty() {
                    None
                } else {
                    Some(username_value)
                }
            });

        let password = selected_certificate
            .as_ref()
            .and_then(|certificate| certificate.password.clone())
            .or_else(|| {
                let password_value = self.password_input.read(cx).text().to_string();
                let password_value = password_value.trim().to_string();
                if password_value.is_empty() {
                    None
                } else {
                    Some(password_value)
                }
            });

        let auth_source_value = self.authentication_source_input.read(cx).text().to_string();
        let auth_source_value = auth_source_value.trim().to_string();
        let auth_source = if auth_source_value.is_empty() {
            None
        } else {
            Some(auth_source_value)
        };

        let replica_set_value = self.replica_set_input.read(cx).text().to_string();
        let replica_set_value = replica_set_value.trim().to_string();
        let replica_set = if replica_set_value.is_empty() {
            None
        } else {
            Some(replica_set_value)
        };

        let read_preference_value = self.read_preference_input.read(cx).text().to_string();
        let read_preference_value = read_preference_value.trim().to_string();
        let read_preference = if read_preference_value.is_empty() {
            None
        } else {
            Some(read_preference_value)
        };

        let connect_timeout_text = self
            .connect_timeout_seconds_input
            .read(cx)
            .text()
            .to_string();
        let connect_timeout_text = connect_timeout_text.trim().to_string();
        let connect_timeout_seconds = if connect_timeout_text.is_empty() {
            None
        } else {
            Some(
                connect_timeout_text
                    .parse::<u64>()
                    .map_err(|_| t!("MongoForm.timeout_must_be_number").to_string())?,
            )
        };

        let application_name_value = self.application_name_input.read(cx).text().to_string();
        let application_name_value = application_name_value.trim().to_string();
        let application_name = if application_name_value.is_empty() {
            None
        } else {
            Some(application_name_value)
        };

        Ok(MongoDBParams {
            connection_string: String::new(),
            host: host_value,
            port: port_value,
            database,
            username,
            password,
            credential_ref: selected_certificate
                .as_ref()
                .map(CertificateReference::from_certificate),
            auth_source,
            replica_set,
            read_preference,
            use_srv_record: self.use_srv_record,
            direct_connection: self.direct_connection,
            use_tls: self.use_tls,
            connect_timeout_seconds,
            application_name,
        })
    }

    fn on_test(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().to_string();
        let parameters = match self.build_parameters(cx) {
            Ok(parameters) => parameters,
            Err(error) => {
                self.is_testing = false;
                self.test_result = Some(Err(error));
                cx.notify();
                return;
            }
        };

        let test_name = if name.is_empty() {
            t!("MongoForm.default_name").to_string()
        } else {
            name
        };

        self.is_testing = true;
        self.test_result = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let test_result: Result<(), String> = Tokio::spawn_result(cx, async move {
                MongoManager::test_parameters(test_name, &parameters)
                    .await
                    .map_err(anyhow::Error::new)
            })
            .await
            .map_err(|error| {
                let detailed = format!("{:#}", error);
                error!("MongoDB 连接测试失败: {}", detailed);
                detailed
            });

            let _ = this.update(cx, |this, cx| {
                this.is_testing = false;
                this.test_result = Some(test_result);
                cx.notify();
            });
        })
        .detach();
    }

    fn on_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parameters = match self.build_parameters(cx) {
            Ok(parameters) => parameters,
            Err(error) => {
                self.test_result = Some(Err(error));
                cx.notify();
                return;
            }
        };
        let name = self.name_input.read(cx).text().to_string();
        let name = if name.is_empty() {
            t!("MongoForm.default_name").to_string()
        } else {
            name
        };

        let workspace_id = self.get_workspace_id(cx);
        let owner_id = if !self.is_editing {
            GlobalCloudUser::get_user(cx).map(|u| u.id)
        } else {
            None
        };
        let remark = {
            let value = self.remark_input.read(cx).text().to_string();
            if value.is_empty() { None } else { Some(value) }
        };
        let sync_enabled = self.sync_enabled;
        let is_editing = self.is_editing;
        let editing_id = self.editing_id;
        let editing_cloud_id = self.editing_cloud_id.clone();
        let editing_last_synced_at = self.editing_last_synced_at;

        let storage = cx
            .global::<one_core::storage::GlobalStorageState>()
            .storage
            .clone();

        cx.spawn(async move |_this, cx| {
            let result = Tokio::spawn_result(cx, async move {
                let repo = storage
                    .get::<one_core::storage::ConnectionRepository>()
                    .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;

                let mut connection = StoredConnection::new_mongodb(name, parameters, workspace_id);
                connection.sync_enabled = sync_enabled;
                connection.remark = remark;
                connection.team_id = None;
                if !is_editing {
                    connection.owner_id = owner_id;
                }

                if is_editing {
                    connection.id = editing_id;
                    connection.cloud_id = editing_cloud_id;
                    connection.last_synced_at = editing_last_synced_at;
                    repo.update(&mut connection)?;
                } else {
                    repo.insert(&mut connection)?;
                }
                Ok::<StoredConnection, anyhow::Error>(connection)
            })
            .await;

            match result {
                Ok(saved_conn) => {
                    let _ = cx.update(|cx| {
                        if let Some(notifier) = get_notifier(cx) {
                            let event = if is_editing {
                                ConnectionDataEvent::ConnectionUpdated {
                                    connection: saved_conn,
                                }
                            } else {
                                ConnectionDataEvent::ConnectionCreated {
                                    connection: saved_conn,
                                }
                            };
                            notifier.update(cx, |_, cx| {
                                cx.emit(event);
                            });
                        }
                    });
                }
                Err(error) => {
                    error!(
                        "{}",
                        t!("MongoForm.save_connection_failed", error = error).to_string()
                    );
                }
            }
        })
        .detach();

        window.remove_window();
    }

    fn render_form_row(&self, label: &str, child: impl IntoElement) -> impl IntoElement {
        h_flex()
            .gap_3()
            .items_center()
            .child(
                div()
                    .w(px(120.0))
                    .text_sm()
                    .text_right()
                    .text_color(app_style::text_muted())
                    .child(label.to_string()),
            )
            .child(div().flex_1().child(child))
    }

    fn styled_input(&self, input: Input) -> Input {
        input.refine_style(&app_style::control_style())
    }

    fn styled_select<D>(&self, select: Select<D>) -> Select<D>
    where
        D: SelectDelegate + 'static,
    {
        select.refine_style(&app_style::control_style())
    }

    fn render_basic_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let use_certificate = self.selected_certificate(cx).is_some();

        v_flex()
            .gap_2()
            .child(self.render_form_row(
                t!("MongoForm.name_label").as_ref(),
                self.styled_input(Input::new(&self.name_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.host_label").as_ref(),
                self.styled_input(Input::new(&self.host_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.port_label").as_ref(),
                self.styled_input(Input::new(&self.port_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.database_label").as_ref(),
                self.styled_input(Input::new(&self.database_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.certificate_label").as_ref(),
                self.styled_select(Select::new(&self.credential_select).w_full()),
            ))
            .child(
                self.render_form_row(
                    t!("MongoForm.username_label").as_ref(),
                    self.styled_input(Input::new(&self.username_input))
                        .disabled(use_certificate),
                ),
            )
            .child(
                self.render_form_row(
                    t!("MongoForm.password_label").as_ref(),
                    self.styled_input(Input::new(&self.password_input))
                        .mask_toggle()
                        .disabled(use_certificate),
                ),
            )
            .child(self.render_form_row(
                t!("MongoForm.auth_source_label").as_ref(),
                self.styled_input(Input::new(&self.authentication_source_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.workspace_label").as_ref(),
                self.styled_select(Select::new(&self.workspace_select).w_full()),
            ))
            .child(
                self.render_form_row(
                    t!("MongoForm.cloud_sync_label").as_ref(),
                    h_flex()
                        .gap_2()
                        .child(
                            Checkbox::new("mongo-sync-enabled")
                                .checked(self.sync_enabled)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sync_enabled = !this.sync_enabled;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(app_style::text_muted())
                                .child(t!("MongoForm.cloud_sync_enabled").to_string()),
                        ),
                ),
            )
    }

    fn render_cluster_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                self.render_form_row(
                    t!("MongoForm.use_srv").as_ref(),
                    Checkbox::new("mongo-use-srv")
                        .checked(self.use_srv_record)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.use_srv_record = !this.use_srv_record;
                            cx.notify();
                        })),
                ),
            )
            .child(
                self.render_form_row(
                    t!("MongoForm.direct_connection").as_ref(),
                    Checkbox::new("mongo-direct-connection")
                        .checked(self.direct_connection)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.direct_connection = !this.direct_connection;
                            cx.notify();
                        })),
                ),
            )
            .child(self.render_form_row(
                t!("MongoForm.replica_set_label").as_ref(),
                self.styled_input(Input::new(&self.replica_set_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.read_preference_label").as_ref(),
                self.styled_input(Input::new(&self.read_preference_input)),
            ))
    }

    fn render_advanced_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                self.render_form_row(
                    t!("MongoForm.tls_label").as_ref(),
                    Checkbox::new("mongo-use-tls")
                        .checked(self.use_tls)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.use_tls = !this.use_tls;
                            cx.notify();
                        })),
                ),
            )
            .child(self.render_form_row(
                t!("MongoForm.connect_timeout_label").as_ref(),
                self.styled_input(Input::new(&self.connect_timeout_seconds_input)),
            ))
            .child(self.render_form_row(
                t!("MongoForm.app_name_label").as_ref(),
                self.styled_input(Input::new(&self.application_name_input)),
            ))
    }

    fn render_remark_tab(&self) -> impl IntoElement {
        v_flex().gap_2().child(self.render_form_row(
            t!("MongoForm.remark_label").as_ref(),
            self.styled_input(Input::new(&self.remark_input)),
        ))
    }
}

impl Focusable for MongoFormWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MongoFormWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_testing = self.is_testing;
        let active_tab = self.active_tab;

        let test_result_element = match &self.test_result {
            Some(Ok(())) => Some(
                div().w_full().px_6().pb_2().child(
                    div()
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(app_style::accent_dim_strong())
                        .text_sm()
                        .text_color(app_style::accent())
                        .child(t!("MongoForm.test_success").to_string()),
                ),
            ),
            Some(Err(error)) => Some(
                div().w_full().px_6().pb_2().child(
                    div()
                        .w_full()
                        .max_h(px(120.0))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(app_style::danger_dim())
                        .overflow_y_scrollbar()
                        .whitespace_normal()
                        .text_sm()
                        .text_color(app_style::danger())
                        .child(error.clone()),
                ),
            ),
            None => None,
        };

        v_flex()
            .justify_center()
            .size_full()
            .bg(app_style::page_bg())
            .child(
                TitleBar::new()
                    .refine_style(&app_style::title_bar_style())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_1()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(app_style::text_primary())
                            .child(self.title.clone()),
                    ),
            )
            .child(
                div().flex().justify_center().px_4().pt_3().pb_1().child(
                    div()
                        .rounded_xl()
                        .border_1()
                        .border_color(app_style::border())
                        .bg(app_style::panel_bg())
                        .px_2()
                        .py_2()
                        .child(
                            TabBar::new("mongodb-form-tabs")
                                .with_size(Size::Small)
                                .underline()
                                .selected_index(active_tab)
                                .on_click(cx.listener(|this, ix: &usize, _, cx| {
                                    this.active_tab = *ix;
                                    cx.notify();
                                }))
                                .child(Tab::new().label(t!("MongoForm.tab_basic").to_string()))
                                .child(Tab::new().label(t!("MongoForm.tab_cluster").to_string()))
                                .child(Tab::new().label(t!("MongoForm.tab_advanced").to_string()))
                                .child(Tab::new().label(t!("MongoForm.tab_remark").to_string())),
                        ),
                ),
            )
            .child(
                div()
                    .id("mongo-form-content")
                    .flex_1()
                    .mx_4()
                    .mb_4()
                    .rounded_xl()
                    .border_1()
                    .border_color(app_style::border())
                    .bg(app_style::panel_bg())
                    .p_4()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        0 => self.render_basic_tab(cx).into_any_element(),
                        1 => self.render_cluster_tab(cx).into_any_element(),
                        2 => self.render_advanced_tab(cx).into_any_element(),
                        3 => self.render_remark_tab().into_any_element(),
                        _ => div().into_any_element(),
                    }),
            )
            .when_some(test_result_element, |this, elem| this.child(elem))
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .px_6()
                    .py_4()
                    .border_t_1()
                    .border_color(app_style::border())
                    .bg(app_style::panel_bg())
                    .child(
                        Button::new("cancel")
                            .small()
                            .with_variant(app_style::secondary_button_variant(cx))
                            .label(t!("Common.cancel").to_string())
                            .on_click(|_, window, _cx| {
                                window.remove_window();
                            }),
                    )
                    .child(
                        Button::new("test")
                            .small()
                            .with_variant(app_style::secondary_button_variant(cx))
                            .icon(IconName::Refresh)
                            .label(if is_testing {
                                t!("MongoForm.testing").to_string()
                            } else {
                                t!("MongoForm.test").to_string()
                            })
                            .disabled(is_testing)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_test(window, cx);
                            })),
                    )
                    .child(
                        Button::new("manage-certificates")
                            .small()
                            .with_variant(app_style::secondary_button_variant(cx))
                            .label(t!("MongoForm.manage_certificates").to_string())
                            .on_click(cx.listener(|_, _, _window, cx| {
                                open_certificate_manager_popup(cx);
                            })),
                    )
                    .child(
                        Button::new("ok")
                            .small()
                            .with_variant(app_style::primary_button_variant(cx))
                            .label(t!("Common.save").to_string())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_save(window, cx);
                            })),
                    ),
            )
    }
}
