use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Sizable, StyledExt as _, TitleBar, app_style,
    button::{Button, ButtonVariant, ButtonVariants},
    h_flex, v_flex,
};
use one_core::llm::{storage::ProviderRepository, types::ProviderConfig};
use one_core::popup_window::{PopupWindowOptions, open_popup_window, request_popup_window_close};
use one_core::storage::{GlobalStorageState, StorageManager, traits::Repository};
use rust_i18n::t;

use super::provider_form_dialog::ProviderForm;

pub struct LlmProvidersView {
    focus_handle: FocusHandle,
    storage_manager: StorageManager,
    providers: Vec<ProviderConfig>,
    loading: bool,
}

impl LlmProvidersView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let storage_state = cx.global::<GlobalStorageState>();
        let storage_manager = storage_state.storage.clone();

        let instance = Self {
            focus_handle,
            storage_manager,
            providers: vec![],
            loading: false,
        };
        cx.spawn(async move |entity: WeakEntity<Self>, cx: &mut AsyncApp| {
            let _ = entity.update(cx, |this, cx| {
                this.load_providers(cx);
            });
        })
        .detach();

        instance
    }

    fn load_providers(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let repo = self
            .storage_manager
            .get::<ProviderRepository>()
            .expect("ProviderRepository not found");

        match repo.list() {
            Ok(providers) => self.providers = providers,
            Err(e) => {
                tracing::error!("Failed to load providers: {}", e);
            }
        }
        self.loading = false;
        cx.notify();
    }

    fn add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_provider_form(None, cx, window);
    }

    fn edit_provider(&mut self, provider_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let provider = self.providers.iter().find(|p| p.id == provider_id).cloned();
        self.open_provider_form(provider, cx, window);
    }

    fn open_provider_form(
        &mut self,
        provider: Option<ProviderConfig>,
        cx: &mut Context<Self>,
        window: &mut Window,
    ) {
        let is_update = provider.is_some();
        let storage_manager = self.storage_manager.clone();
        let view = cx.entity().clone();

        open_popup_window(
            window,
            PopupWindowOptions::new(if is_update {
                t!("LlmProviders.dialog_edit_title").to_string()
            } else {
                t!("LlmProviders.dialog_add_title").to_string()
            })
            .size(560.0, 560.0),
            move |window, cx| {
                cx.new(|cx| ProviderEditorView::new(provider, storage_manager, view, window, cx))
            },
            cx,
        );
    }

    fn delete_provider(&mut self, provider_id: i64, cx: &mut Context<Self>) {
        let repo = self
            .storage_manager
            .get::<ProviderRepository>()
            .expect("ProviderRepository not found");

        match repo.delete(provider_id) {
            Ok(_) => self.load_providers(cx),
            Err(e) => tracing::error!("Failed to delete provider: {}", e),
        }
    }

    fn toggle_default(&mut self, provider: &ProviderConfig, cx: &mut Context<Self>) {
        let repo = self
            .storage_manager
            .get::<ProviderRepository>()
            .expect("ProviderRepository not found");

        let new_default = !provider.is_default;

        // 取消所有其他 provider 的默认标记
        if new_default {
            if let Ok(existing) = repo.list() {
                for mut item in existing {
                    if item.id != provider.id && item.is_default {
                        item.is_default = false;
                        if let Err(e) = repo.update(&item) {
                            tracing::error!("Failed to unset default provider: {}", e);
                        }
                    }
                }
            }
        }

        let mut updated = provider.clone();
        updated.is_default = new_default;

        match repo.update(&updated) {
            Ok(_) => self.load_providers(cx),
            Err(e) => tracing::error!("Failed to toggle default provider: {}", e),
        }
    }

    fn toggle_provider(&mut self, provider: &ProviderConfig, cx: &mut Context<Self>) {
        let mut updated = provider.clone();
        updated.enabled = !updated.enabled;

        let repo = self
            .storage_manager
            .get::<ProviderRepository>()
            .expect("ProviderRepository not found");

        match repo.update(&updated) {
            Ok(_) => self.load_providers(cx),
            Err(e) => tracing::error!("Failed to toggle provider: {}", e),
        }
    }
}

impl Render for LlmProvidersView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_4()
            .p_6()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(t!("LlmProviders.title").to_string()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t!("LlmProviders.description").to_string()),
                            ),
                    )
                    .child(
                        Button::new("add-provider")
                            .with_variant(ButtonVariant::Primary)
                            .label(t!("LlmProviders.add_provider"))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.add_provider(window, cx);
                            })),
                    ),
            )
            .child(if self.loading {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(t!("LlmProviders.loading").to_string())
                    .into_any_element()
            } else if self.providers.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        v_flex()
                            .gap_2()
                            .items_center()
                            .child(t!("LlmProviders.empty_title").to_string())
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t!("LlmProviders.empty_description").to_string()),
                            ),
                    )
                    .into_any_element()
            } else {
                let mut cards = v_flex().gap_3();
                for provider in &self.providers {
                    cards = cards.child(self.render_provider_card(provider, cx));
                }
                cards.into_any_element()
            })
    }
}

impl LlmProvidersView {
    fn render_provider_card(
        &self,
        provider: &ProviderConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let provider_id = provider.id;
        let provider_for_default = provider.clone();
        let provider_for_toggle = provider.clone();

        let info = v_flex()
            .flex_1()
            .gap_2()
            .child(self.render_provider_header(provider, cx))
            .child(self.render_provider_details(provider, cx));

        let actions = self
            .render_provider_actions(provider_id, &provider_for_toggle, &provider_for_default, cx)
            .into_any_element();

        div()
            .flex()
            .p_4()
            .gap_4()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            // Layer 5: 卡片 - 0.18
            .bg(cx.theme().background)
            .child(info)
            .child(actions)
    }

    fn render_provider_header(
        &self,
        provider: &ProviderConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(provider.name.clone()),
            )
            .child(
                div()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .text_xs()
                    .child(provider.provider_type.display_name()),
            )
            .when(provider.is_default, |this| {
                this.child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded_md()
                        .bg(cx.theme().primary)
                        .text_xs()
                        .text_color(cx.theme().primary_foreground)
                        .child(t!("LlmProviders.default_label").to_string()),
                )
            })
            .when(!provider.enabled, |this| {
                this.child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded_md()
                        .bg(cx.theme().muted)
                        .text_xs()
                        .child(t!("LlmProviders.disabled_label").to_string()),
                )
            })
    }

    fn render_provider_details(
        &self,
        provider: &ProviderConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let api_base_display = provider.api_base.clone();
        let api_version_display = provider.api_version.clone();

        v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        t!(
                            "LlmProviders.default_model",
                            model = provider.model.as_str()
                        )
                        .to_string(),
                    ),
            )
            .when(!provider.models.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            t!(
                                "LlmProviders.optional_models_count",
                                count = provider.models.len()
                            )
                            .to_string(),
                        ),
                )
            })
            .when_some(api_base_display, |this, base| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("LlmProviders.api_base", value = base.as_str()).to_string()),
                )
            })
            .when_some(api_version_display, |this, version| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            t!("LlmProviders.api_version", value = version.as_str()).to_string(),
                        ),
                )
            })
    }

    /// 设置页中的 provider 统一支持启用/禁用、设置默认、编辑、删除
    fn render_provider_actions(
        &self,
        provider_id: i64,
        provider_for_toggle: &ProviderConfig,
        provider_for_default: &ProviderConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let toggle_clone = provider_for_toggle.clone();
        let default_clone = provider_for_default.clone();
        let is_enabled = provider_for_toggle.enabled;
        let is_default = provider_for_default.is_default;

        h_flex()
            .gap_2()
            .items_center()
            .child(
                Button::new(SharedString::from(format!("default-{}", provider_id)))
                    .with_variant(if is_default {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Primary
                    })
                    .label(if is_default {
                        t!("LlmProviders.action_unset_default")
                    } else {
                        t!("LlmProviders.action_set_default")
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_default(&default_clone, cx);
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("toggle-{}", provider_id)))
                    .with_variant(ButtonVariant::Secondary)
                    .label(if is_enabled {
                        t!("LlmProviders.action_disable")
                    } else {
                        t!("LlmProviders.action_enable")
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_provider(&toggle_clone, cx);
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("edit-{}", provider_id)))
                    .with_variant(ButtonVariant::Secondary)
                    .label(t!("LlmProviders.action_edit"))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.edit_provider(provider_id, window, cx);
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("delete-{}", provider_id)))
                    .with_variant(ButtonVariant::Secondary)
                    .label(t!("LlmProviders.action_delete"))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.delete_provider(provider_id, cx);
                    })),
            )
    }
}

impl Focusable for LlmProvidersView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for LlmProvidersView {}

struct ProviderEditorView {
    focus_handle: FocusHandle,
    form: Entity<ProviderForm>,
    is_update: bool,
    storage_manager: StorageManager,
    view: WeakEntity<LlmProvidersView>,
    is_saving: bool,
    error_message: Option<String>,
}

impl ProviderEditorView {
    fn new(
        provider: Option<ProviderConfig>,
        storage_manager: StorageManager,
        view: Entity<LlmProvidersView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let is_update = provider.is_some();
        let form = cx.new(|cx| ProviderForm::new_with_config(provider, window, cx));

        Self {
            focus_handle: cx.focus_handle(),
            form,
            is_update,
            storage_manager,
            view: view.downgrade(),
            is_saving: false,
            error_message: None,
        }
    }

    fn on_cancel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut config) = self.form.update(
            cx,
            |form: &mut ProviderForm, cx: &mut Context<ProviderForm>| form.get_config(cx),
        ) else {
            self.error_message = Some(t!("LlmProviders.required_notice").to_string());
            cx.notify();
            return;
        };

        self.is_saving = true;
        self.error_message = None;
        cx.notify();

        let repo = self
            .storage_manager
            .get::<ProviderRepository>()
            .expect("ProviderRepository not found");

        if config.is_default {
            if let Ok(existing) = repo.list() {
                for mut item in existing {
                    if item.id != config.id && item.is_default {
                        item.is_default = false;
                        if let Err(e) = repo.update(&item) {
                            tracing::error!("Failed to unset default provider: {}", e);
                        }
                    }
                }
            }
        }

        let result = if self.is_update {
            repo.update(&config)
        } else {
            repo.insert(&mut config).map(|_| ())
        };

        self.is_saving = false;

        match result {
            Ok(_) => {
                if let Some(view) = self.view.upgrade() {
                    let _ = view.update(cx, |view, cx| {
                        view.load_providers(cx);
                    });
                }
                request_popup_window_close(window, cx);
            }
            Err(e) => {
                tracing::error!("Failed to save provider: {}", e);
                cx.notify();
            }
        }
    }
}

impl Focusable for ProviderEditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ProviderEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_saving = self.is_saving;
        let title = if self.is_update {
            t!("LlmProviders.dialog_edit_title").to_string()
        } else {
            t!("LlmProviders.dialog_add_title").to_string()
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
                    .id("provider-form-content")
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
                        Button::new("provider-editor-cancel")
                            .small()
                            .with_variant(app_style::secondary_button_variant(cx))
                            .label(t!("Common.cancel").to_string())
                            .disabled(is_saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("provider-editor-save")
                            .small()
                            .with_variant(app_style::primary_button_variant(cx))
                            .label(if self.is_update {
                                t!("Common.save").to_string()
                            } else {
                                t!("LlmProviders.dialog_add_action").to_string()
                            })
                            .disabled(is_saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_save(window, cx);
                            })),
                    ),
            )
    }
}
