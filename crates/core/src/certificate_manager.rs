use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme, Disableable, IndexPath, Sizable, StyledExt as _, WindowExt, app_style,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectItem, SelectState},
    v_flex, TitleBar
};
use rust_i18n::t;

use crate::certificate_notifier::{CertificateDataEvent, emit_certificate_event};
use crate::cloud_sync::GlobalCloudUser;
use crate::connection_notifier::{ConnectionDataEvent, emit_connection_event};
use crate::gpui_tokio::Tokio;
use crate::popup_window::{PopupWindowOptions, open_popup_window, request_popup_window_close};
use crate::storage::traits::Repository;
use crate::storage::{
    Certificate, CertificateKind, CertificateRepository, GlobalStorageState,
    PendingCloudDeletionRepository, detach_connections_for_certificate,
    sync_connections_for_certificate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CertificateKindItem {
    kind: CertificateKind,
}

impl CertificateKindItem {
    fn new(kind: CertificateKind) -> Self {
        Self { kind }
    }
}

impl SelectItem for CertificateKindItem {
    type Value = CertificateKind;

    fn title(&self) -> SharedString {
        self.kind.label().into()
    }

    fn value(&self) -> &Self::Value {
        &self.kind
    }
}

pub struct CertificateManagerView {
    focus_handle: FocusHandle,
    certificates: Vec<Certificate>,
    _subscriptions: Vec<Subscription>,
}

impl CertificateManagerView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            focus_handle: cx.focus_handle(),
            certificates: Vec::new(),
            _subscriptions: Vec::new(),
        };
        if let Some(notifier) = crate::certificate_notifier::get_notifier(cx) {
            view._subscriptions.push(cx.subscribe(
                &notifier,
                |this, _, _event: &CertificateDataEvent, cx| {
                    this.load_certificates(cx);
                },
            ));
        }
        view.load_certificates(cx);
        view
    }

    fn load_certificates(&mut self, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let Some(repo) = storage.get::<CertificateRepository>() else {
            self.certificates.clear();
            cx.notify();
            return;
        };

        match repo.list() {
            Ok(certificates) => {
                self.certificates = certificates;
            }
            Err(error) => {
                tracing::error!("加载证书失败: {}", error);
                self.certificates.clear();
            }
        }

        cx.notify();
    }

    fn confirm_delete(
        &mut self,
        certificate: Certificate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let certificate = certificate.clone();
            let view = view.clone();
            dialog
                .title(t!("Common.delete").to_string().into_any_element())
                .child(
                    t!(
                        "CertificateManager.delete_confirm",
                        name = certificate.name.clone()
                    )
                    .to_string()
                    .into_any_element(),
                )
                .confirm()
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.delete_certificate(&certificate, cx);
                    });
                    true
                })
        });
    }

    fn delete_certificate(&mut self, certificate: &Certificate, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let Some(repo) = storage.get::<CertificateRepository>() else {
            return;
        };

        if let Some(cloud_id) = &certificate.cloud_id {
            if let Some(pending_repo) = storage.get::<PendingCloudDeletionRepository>() {
                if let Err(error) = pending_repo.add(cloud_id, "certificate") {
                    tracing::error!("记录证书待删除同步失败: {}", error);
                }
            }
        }

        match detach_connections_for_certificate(
            &storage,
            certificate.id,
            certificate.cloud_id.as_deref(),
        ) {
            Ok(changed_connections) => {
                if let Some(id) = certificate.id {
                    if let Err(error) = repo.delete(id) {
                        tracing::error!("删除证书失败: {}", error);
                        return;
                    }
                }

                for connection in changed_connections {
                    emit_connection_event(
                        ConnectionDataEvent::ConnectionUpdated { connection },
                        cx,
                    );
                }

                if let Some(id) = certificate.id {
                    emit_certificate_event(
                        CertificateDataEvent::Deleted { certificate_id: id },
                        cx,
                    );
                }
            }
            Err(error) => {
                tracing::error!("解除证书引用失败: {}", error);
                return;
            }
        }

        self.load_certificates(cx);
    }
}

impl Focusable for CertificateManagerView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CertificateManagerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary_btn_variant = app_style::primary_button_variant(cx);

        let certificate_cards =
            v_flex()
                .gap_3()
                .children(self.certificates.iter().cloned().map(|certificate| {
                    let edit_certificate = certificate.clone();
                    let delete_certificate = certificate.clone();
                    let subtitle = certificate.display_subtitle();
                    let remark = certificate.remark.clone();
                    let sync_text = if certificate.sync_enabled {
                        t!("CertificateManager.sync_enabled").to_string()
                    } else {
                        t!("CertificateManager.sync_disabled").to_string()
                    };

                    v_flex()
                        .gap_1()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(app_style::border())
                        .bg(cx.theme().group)
                        .child(
                            h_flex()
                                .justify_between()
                                .items_start()
                                .gap_1()
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(app_style::text())
                                                .child(certificate.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(app_style::text_muted())
                                                .child(certificate.kind.label().to_string()),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(app_style::text_muted())
                                                .child(subtitle),
                                        )
                                        .when_some(remark, |this, remark| {
                                            this.child(
                                                div()
                                                    .text_sm()
                                                    .text_color(app_style::text_muted())
                                                    .child(remark),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(app_style::text_muted())
                                                .child(sync_text),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Button::new(format!(
                                                "edit-certificate-{}",
                                                certificate.id.unwrap_or_default()
                                            ))
                                            .small()
                                            .with_variant(app_style::secondary_button_variant(cx))
                                            .label(t!("Common.edit").to_string())
                                            .on_click(cx.listener(move |_, _, window, cx| {
                                                open_certificate_editor_popup(
                                                    Some(edit_certificate.clone()),
                                                    window,
                                                    cx,
                                                );
                                            })),
                                        )
                                        .child(
                                            Button::new(format!(
                                                "delete-certificate-{}",
                                                certificate.id.unwrap_or_default()
                                            ))
                                            .small()
                                            .with_variant(app_style::danger_button_variant(cx))
                                            .label(t!("Common.delete").to_string())
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                view.confirm_delete(
                                                    delete_certificate.clone(),
                                                    window,
                                                    cx,
                                                );
                                            })),
                                        ),
                                ),
                        )
                }));

        v_flex()
            .size_full()
            .child(
                div()
                    .refine_style(&app_style::page_header_style())
                    .border_b_1()
                    .border_color(app_style::border())
                    .p_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_1()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(app_style::text())
                                            .child(t!("CertificateManager.title").to_string()),
                                    )
                                    .child(
                                        div().text_sm().text_color(app_style::text_muted()).child(
                                            t!("CertificateManager.description").to_string(),
                                        ),
                                    ),
                            )
                            .child(
                                Button::new("add-certificate")
                                    .with_variant(primary_btn_variant)
                                    .label(t!("CertificateManager.add").to_string())
                                    .on_click(cx.listener(|_, _, window, cx| {
                                        open_certificate_editor_popup(None, window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .overflow_y_scrollbar()
                    .when(self.certificates.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(app_style::text_muted())
                                .child(t!("CertificateManager.empty").to_string()),
                        )
                    })
                    .when(!self.certificates.is_empty(), |this| {
                        this.child(certificate_cards)
                    }),
            )
    }
}

struct CertificateForm {
    focus_handle: FocusHandle,
    original: Option<Certificate>,
    name_input: Entity<InputState>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    key_path_input: Entity<InputState>,
    passphrase_input: Entity<InputState>,
    remark_input: Entity<InputState>,
    kind_select: Entity<SelectState<Vec<CertificateKindItem>>>,
    sync_enabled: bool,
}

impl CertificateForm {
    fn new(certificate: Option<Certificate>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| {
            let mut state =
                InputState::new(window, cx).placeholder(t!("CertificateManager.name_placeholder"));
            if let Some(certificate) = &certificate {
                state.set_value(certificate.name.clone(), window, cx);
            }
            state
        });
        let username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("CertificateManager.username_placeholder"));
            if let Some(certificate) = &certificate {
                if let Some(username) = certificate.username() {
                    state.set_value(username.to_string(), window, cx);
                }
            }
            state
        });
        let password_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("CertificateManager.password_placeholder"))
                .masked(true);
            if let Some(certificate) = &certificate {
                if let Some(password) = certificate.password() {
                    state.set_value(password.to_string(), window, cx);
                }
            }
            state
        });
        let key_path_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("CertificateManager.key_path_placeholder"));
            if let Some(certificate) = &certificate {
                if let Some(key_path) = certificate.key_path() {
                    state.set_value(key_path.to_string(), window, cx);
                }
            }
            state
        });
        let passphrase_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("CertificateManager.passphrase_placeholder"))
                .masked(true);
            if let Some(certificate) = &certificate {
                if let Some(passphrase) = certificate.passphrase() {
                    state.set_value(passphrase.to_string(), window, cx);
                }
            }
            state
        });
        let remark_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("CertificateManager.remark_placeholder"))
                .auto_grow(3, 8);
            if let Some(certificate) = &certificate {
                if let Some(remark) = &certificate.remark {
                    state.set_value(remark.clone(), window, cx);
                }
            }
            state
        });

        let kind_items = vec![
            CertificateKindItem::new(CertificateKind::UsernamePassword),
            CertificateKindItem::new(CertificateKind::SshPrivateKey),
        ];
        let selected_kind = certificate
            .as_ref()
            .map(|certificate| certificate.kind)
            .unwrap_or(CertificateKind::UsernamePassword);
        let selected_kind_index = kind_items
            .iter()
            .position(|item| item.kind == selected_kind)
            .map(IndexPath::new)
            .or(Some(IndexPath::new(0)));
        let kind_select =
            cx.new(|cx| SelectState::new(kind_items, selected_kind_index, window, cx));

        Self {
            focus_handle: cx.focus_handle(),
            original: certificate.clone(),
            name_input,
            username_input,
            password_input,
            key_path_input,
            passphrase_input,
            remark_input,
            kind_select,
            sync_enabled: certificate
                .map(|certificate| certificate.sync_enabled)
                .unwrap_or(true),
        }
    }

    fn selected_kind(&self, cx: &App) -> CertificateKind {
        self.kind_select
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or(CertificateKind::UsernamePassword)
    }

    fn build_certificate(&self, cx: &App) -> Option<Certificate> {
        let name = self
            .name_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .to_string();
        let username = self
            .username_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .to_string();
        if name.is_empty() || username.is_empty() {
            return None;
        }

        let kind = self.selected_kind(cx);
        let password = {
            let value = self.password_input.read(cx).text().to_string();
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        };
        let key_path = {
            let value = self
                .key_path_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string();
            if value.is_empty() { None } else { Some(value) }
        };
        let passphrase = {
            let value = self.passphrase_input.read(cx).text().to_string();
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        };
        if kind == CertificateKind::SshPrivateKey && key_path.is_none() {
            return None;
        }

        let remark = {
            let value = self
                .remark_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string();
            if value.is_empty() { None } else { Some(value) }
        };

        // Build params JSON
        let mut params_map = serde_json::Map::new();
        params_map.insert(
            "username".to_string(),
            serde_json::Value::String(username.clone()),
        );
        if kind == CertificateKind::UsernamePassword {
            if let Some(ref p) = password {
                params_map.insert("password".to_string(), serde_json::Value::String(p.clone()));
            }
        }
        if kind == CertificateKind::SshPrivateKey {
            if let Some(ref kp) = key_path {
                params_map.insert(
                    "key_path".to_string(),
                    serde_json::Value::String(kp.clone()),
                );
            }
            if let Some(ref ph) = passphrase {
                params_map.insert(
                    "passphrase".to_string(),
                    serde_json::Value::String(ph.clone()),
                );
            }
        }
        let params = serde_json::Value::Object(params_map);

        let mut certificate = self.original.clone().unwrap_or(Certificate {
            id: None,
            name: String::new(),
            kind,
            params,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: GlobalCloudUser::get_user(cx).map(|user| user.id),
        });

        certificate.name = name;
        certificate.kind = kind;
        // Update params by mutating the JSON
        if let Some(obj) = certificate.params.as_object_mut() {
            obj.insert("username".to_string(), serde_json::Value::String(username));
            match kind {
                CertificateKind::UsernamePassword => {
                    if let Some(ref p) = password {
                        obj.insert("password".to_string(), serde_json::Value::String(p.clone()));
                    } else {
                        obj.remove("password");
                    }
                    obj.remove("key_path");
                    obj.remove("passphrase");
                }
                CertificateKind::SshPrivateKey => {
                    obj.remove("password");
                    if let Some(ref kp) = key_path {
                        obj.insert(
                            "key_path".to_string(),
                            serde_json::Value::String(kp.clone()),
                        );
                    }
                    if let Some(ref ph) = passphrase {
                        obj.insert(
                            "passphrase".to_string(),
                            serde_json::Value::String(ph.clone()),
                        );
                    } else {
                        obj.remove("passphrase");
                    }
                }
            }
        }
        certificate.remark = remark;
        certificate.sync_enabled = self.sync_enabled;

        Some(certificate)
    }
}

impl Focusable for CertificateForm {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CertificateForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_kind = self.selected_kind(cx);

        v_flex()
            .gap_1()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(t!("CertificateManager.name").to_string()),
                    )
                    .child(Input::new(&self.name_input).w_full()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(t!("CertificateManager.kind").to_string()),
                    )
                    .child(Select::new(&self.kind_select).w_full()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(t!("CertificateManager.username").to_string()),
                    )
                    .child(Input::new(&self.username_input).w_full()),
            )
            .when(selected_kind == CertificateKind::UsernamePassword, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("CertificateManager.password").to_string()),
                        )
                        .child(
                            Input::new(&self.password_input)
                                .w_full()
                                .mask_toggle()
                                .disable_ime(),
                        ),
                )
            })
            .when(selected_kind == CertificateKind::SshPrivateKey, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("CertificateManager.key_path").to_string()),
                        )
                        .child(Input::new(&self.key_path_input).w_full()),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("CertificateManager.passphrase").to_string()),
                        )
                        .child(
                            Input::new(&self.passphrase_input)
                                .w_full()
                                .mask_toggle()
                                .disable_ime(),
                        ),
                )
            })
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Checkbox::new("certificate-sync-enabled")
                            .checked(self.sync_enabled)
                            .on_click(cx.listener(|form, _, _window, cx| {
                                form.sync_enabled = !form.sync_enabled;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(t!("CertificateManager.sync_switch").to_string()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(t!("CertificateManager.remark").to_string()),
                    )
                    .child(Input::new(&self.remark_input).w_full()),
            )
    }
}

struct CertificateEditorView {
    focus_handle: FocusHandle,
    form: Entity<CertificateForm>,
    is_editing: bool,
    is_saving: bool,
    error_message: Option<String>,
}

impl CertificateEditorView {
    fn new(certificate: Option<Certificate>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let is_editing = certificate.is_some();
        let certificate_for_form = certificate.clone();
        let form = cx.new(move |cx| CertificateForm::new(certificate_for_form, window, cx));

        Self {
            focus_handle: cx.focus_handle(),
            form,
            is_editing,
            is_saving: false,
            error_message: None,
        }
    }

    fn on_cancel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut certificate) = self.form.update(cx, |form, cx| form.build_certificate(cx))
        else {
            self.error_message = Some(t!("CertificateManager.validation_error").to_string());
            cx.notify();
            return;
        };

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let is_editing = self.is_editing;
        let view = cx.entity().downgrade();

        self.is_saving = true;
        self.error_message = None;
        cx.notify();

        window
            .spawn(cx, async move |cx| {
                let result = Tokio::spawn_result(cx, async move {
                    let repo = storage.get::<CertificateRepository>().ok_or_else(|| {
                        anyhow::anyhow!(t!("CertificateManager.repository_unavailable").to_string())
                    })?;

                    if is_editing {
                        repo.update(&certificate)?;
                    } else {
                        repo.insert(&mut certificate).map(|_| ())?;
                    }

                    let changed_connections =
                        sync_connections_for_certificate(&storage, &certificate)?;
                    Ok::<_, anyhow::Error>((certificate, changed_connections))
                })
                .await;

                if let Some(view) = view.upgrade() {
                    let _ = view.update_in(cx, |this, window, cx| {
                        this.is_saving = false;

                        match result {
                            Ok((certificate, changed_connections)) => {
                                for connection in changed_connections {
                                    emit_connection_event(
                                        ConnectionDataEvent::ConnectionUpdated { connection },
                                        cx,
                                    );
                                }
                                emit_certificate_event(
                                    CertificateDataEvent::Saved {
                                        certificate: certificate.clone(),
                                    },
                                    cx,
                                );
                                request_popup_window_close(window, cx);
                            }
                            Err(error) => {
                                tracing::error!("保存证书失败: {}", error);
                                this.error_message = Some(
                                    t!("CertificateManager.save_failed", error = error.to_string())
                                        .to_string(),
                                );
                                cx.notify();
                            }
                        }
                    });
                }
            })
            .detach();
    }
}

impl Focusable for CertificateEditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CertificateEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_saving = self.is_saving;
        let title = if self.is_editing {
            t!("CertificateManager.edit_title").to_string()
        } else {
            t!("CertificateManager.add_title").to_string()
        };

        let error_element = self.error_message.as_ref().map(|error_message| {
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(app_style::danger_dim())
                .text_sm()
                .text_color(app_style::danger())
                .child(error_message.clone())
        });

        v_flex()
            .justify_center()
            .size_full()
            .rounded(cx.theme().radius_lg)
            .bg(app_style::base())
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
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(app_style::text())
                            .child(title),
                    ),
            )
            .child(
                div()
                    .id("certificate-form-content")
                    .flex_1()
                    .p_4()
                    .overflow_y_scroll()
                    .child(self.form.clone()),
            )
            .when_some(error_element, |this, elem| {
                this.child(h_flex().justify_center().pb_2().child(elem))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .px_6()
                    .py_4()
                    .border_t_1()
                    .border_color(app_style::border())
                    .bg(app_style::surface())
                    .rounded_bl(cx.theme().radius_lg)
                    .rounded_br(cx.theme().radius_lg)
                    .child(
                        Button::new("certificate-editor-cancel")
                            .small()
                            .with_variant(app_style::secondary_button_variant(cx))
                            .label(t!("Common.cancel").to_string())
                            .disabled(is_saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("certificate-editor-save")
                            .small()
                            .with_variant(app_style::primary_button_variant(cx))
                            .label(if self.is_editing {
                                t!("Common.save").to_string()
                            } else {
                                t!("CertificateManager.add_action").to_string()
                            })
                            .disabled(is_saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_save(window, cx);
                            })),
                    ),
            )
    }
}

fn open_certificate_editor_popup(
    certificate: Option<Certificate>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = if certificate.is_some() {
        t!("CertificateManager.edit_title").to_string()
    } else {
        t!("CertificateManager.add_title").to_string()
    };

    open_popup_window(
        window,
        PopupWindowOptions::new(title).size(560.0, 460.0),
        move |window, cx| {
            let certificate = certificate.clone();
            cx.new(|cx| CertificateEditorView::new(certificate, window, cx))
        },
        cx,
    );
}

pub fn open_certificate_manager_popup(window: &mut Window, cx: &mut App) {
    open_popup_window(
        window,
        PopupWindowOptions::new(t!("CertificateManager.window_title").to_string())
            .size(560.0, 460.0),
        |_window, cx| cx.new(|cx| CertificateManagerView::new(cx)),
        cx,
    );
}
