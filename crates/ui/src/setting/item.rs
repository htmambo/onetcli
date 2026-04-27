use gpui::{
    AnyElement, App, Axis, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, prelude::FluentBuilder as _,
};
use std::{any::TypeId, cell::RefCell, ops::Deref, rc::Rc};

use crate::{
    ActiveTheme as _, AxisExt, StyledExt as _,
    label::Label,
    setting::{
        AnySettingField, ElementField, RenderOptions,
        fields::{BoolField, DropdownField, NumberField, SettingFieldRender, StringField},
    },
    text::Text,
    v_flex,
};

/// Setting item.
#[derive(Clone)]
pub enum SettingItem {
    /// A normal setting item with a title, description, and field.
    Item {
        title: SharedString,
        description: Option<Text>,
        layout: Axis,
        field: Rc<dyn AnySettingField>,
        /// Optional visibility filter; if present, item is only shown when this returns true.
        visibility_filter: Option<Rc<dyn Fn(&App) -> bool>>,
    },
    /// A full custom element to render.
    Element {
        render: Rc<dyn Fn(&RenderOptions, &mut Window, &mut App) -> AnyElement + 'static>,
    },
    /// A custom element with an associated action callback.
    ActionButton {
        render: Rc<dyn Fn(&RenderOptions, &mut Window, &mut App) -> AnyElement + 'static>,
        on_click: Rc<RefCell<Box<dyn FnMut(&mut Window, &mut App)>>>,
        visibility_filter: Option<Rc<dyn Fn(&App) -> bool>>,
    },
}

impl SettingItem {
    /// Create a new setting item.
    pub fn new<F>(title: impl Into<SharedString>, field: F) -> Self
    where
        F: AnySettingField + 'static,
    {
        SettingItem::Item {
            title: title.into(),
            description: None,
            layout: Axis::Horizontal,
            field: Rc::new(field),
            visibility_filter: None,
        }
    }

    /// Set a visibility filter for this setting item.
    /// The item is only shown when `filter(cx)` returns true.
    ///
    /// Only applies to [`SettingItem::Item`] and [`SettingItem::ActionButton`].
    pub fn visible_when<F>(mut self, filter: F) -> Self
    where
        F: Fn(&App) -> bool + 'static,
    {
        match &mut self {
            SettingItem::Item {
                visibility_filter, ..
            } => {
                *visibility_filter = Some(Rc::new(filter));
            }
            SettingItem::ActionButton {
                visibility_filter, ..
            } => {
                *visibility_filter = Some(Rc::new(filter));
            }
            SettingItem::Element { .. } => {}
        }
        self
    }

    /// Create a new custom element setting item with a render closure.
    pub fn render<R, E>(render: R) -> Self
    where
        E: IntoElement,
        R: Fn(&RenderOptions, &mut Window, &mut App) -> E + 'static,
    {
        SettingItem::Element {
            render: Rc::new(move |options, window, cx| {
                render(options, window, cx).into_any_element()
            }),
        }
    }

    /// Create a custom element setting item that also receives an action callback.
    /// The `on_click` is called with `&mut Window, &mut App` so it can open dialogs/sheets.
    pub fn action_button<R, E>(
        render: R,
        on_click: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Self
    where
        E: IntoElement,
        R: Fn(&RenderOptions, &mut Window, &mut App) -> E + 'static,
    {
        SettingItem::ActionButton {
            render: Rc::new(move |options, window, cx| {
                render(options, window, cx).into_any_element()
            }),
            on_click: Rc::new(RefCell::new(Box::new(on_click))),
            visibility_filter: None,
        }
    }

    /// Set the description of the setting item.
    ///
    /// Only applies to [`SettingItem::Item`].
    pub fn description(mut self, description: impl Into<Text>) -> Self {
        match &mut self {
            SettingItem::Item { description: d, .. } => {
                *d = Some(description.into());
            }
            SettingItem::Element { .. } | SettingItem::ActionButton { .. } => {}
        }
        self
    }

    /// Set the layout of the setting item.
    ///
    /// Only applies to [`SettingItem::Item`].
    pub fn layout(mut self, layout: Axis) -> Self {
        match &mut self {
            SettingItem::Item { layout: l, .. } => {
                *l = layout;
            }
            SettingItem::Element { .. } | SettingItem::ActionButton { .. } => {}
        }
        self
    }

    pub(crate) fn is_match(&self, query: &str, cx: &App) -> bool {
        match self {
            SettingItem::Item {
                title,
                description,
                visibility_filter,
                ..
            } => {
                // Check visibility filter first
                if let Some(filter) = visibility_filter {
                    if !filter(cx) {
                        return false;
                    }
                }
                // Then check search query
                let query_match = query.is_empty()
                    || title.to_lowercase().contains(&query.to_lowercase())
                    || description.as_ref().map_or(false, |d| {
                        d.get_text(cx)
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                    });
                query_match
            }
            SettingItem::Element { .. } => query.is_empty(),
            SettingItem::ActionButton {
                visibility_filter, ..
            } => {
                if let Some(filter) = visibility_filter {
                    if !filter(cx) {
                        return false;
                    }
                }
                query.is_empty()
            }
        }
    }

    pub(crate) fn is_resettable(&self, cx: &App) -> bool {
        match self {
            SettingItem::Item {
                field,
                visibility_filter,
                ..
            } => {
                if let Some(filter) = visibility_filter {
                    if !filter(cx) {
                        return false;
                    }
                }
                field.is_resettable(cx)
            }
            SettingItem::Element { .. } => false,
            SettingItem::ActionButton {
                visibility_filter, ..
            } => {
                if let Some(filter) = visibility_filter {
                    if !filter(cx) {
                        return false;
                    }
                }
                false
            }
        }
    }

    pub(crate) fn reset(&self, window: &mut Window, cx: &mut App) {
        match self {
            SettingItem::Item { field, .. } => field.reset(window, cx),
            SettingItem::Element { .. } | SettingItem::ActionButton { .. } => {}
        }
    }

    fn render_field(
        field: Rc<dyn AnySettingField>,
        options: RenderOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let field_type = field.field_type();
        let style = field.style().clone();
        let type_id = field.deref().type_id();
        let renderer: Box<dyn SettingFieldRender> = match type_id {
            t if t == std::any::TypeId::of::<bool>() => {
                Box::new(BoolField::new(field_type.is_switch()))
            }
            t if t == TypeId::of::<f64>() && field_type.is_number_input() => {
                Box::new(NumberField::new(field_type.number_input_options()))
            }
            t if t == TypeId::of::<SharedString>() && field_type.is_input() => {
                Box::new(StringField::<SharedString>::new())
            }
            t if t == TypeId::of::<String>() && field_type.is_input() => {
                Box::new(StringField::<String>::new())
            }
            t if t == TypeId::of::<SharedString>() && field_type.is_dropdown() => Box::new(
                DropdownField::<SharedString>::new(field_type.dropdown_options()),
            ),
            t if t == TypeId::of::<String>() && field_type.is_dropdown() => {
                Box::new(DropdownField::<String>::new(field_type.dropdown_options()))
            }
            _ if field_type.is_element() => Box::new(ElementField::new(field_type.element())),
            _ => unimplemented!("Unsupported setting type: {}", field.deref().type_name()),
        };

        renderer.render(field, &options, &style, window, cx)
    }

    pub(super) fn render_item(
        self,
        options: &RenderOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement + use<> {
        match self {
            SettingItem::Item { .. }
            | SettingItem::Element { .. }
            | SettingItem::ActionButton { .. } => div()
                .id(SharedString::from(format!("item-{}", options.item_ix)))
                .w_full()
                .child(Self::render_item_content(self, options, window, cx))
                .into_any_element(),
        }
    }

    fn render_item_content(
        self,
        options: &RenderOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        match self {
            SettingItem::Item {
                title,
                description,
                layout,
                field,
                visibility_filter: _,
            } => div()
                .w_full()
                .overflow_hidden()
                .map(|this| {
                    if layout.is_horizontal() {
                        this.h_flex().justify_between().items_start()
                    } else {
                        this.v_flex()
                    }
                })
                .gap_3()
                .child(
                    v_flex()
                        .map(|this| {
                            if layout.is_horizontal() {
                                this.flex_1().max_w_3_5()
                            } else {
                                this.w_full()
                            }
                        })
                        .gap_1()
                        .child(Label::new(title.clone()).text_sm())
                        .when_some(description.clone(), |this, description| {
                            this.child(
                                div()
                                    .size_full()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description),
                            )
                        }),
                )
                .child(div().id("field").child(Self::render_field(
                    field,
                    RenderOptions { layout, ..*options },
                    window,
                    cx,
                )))
                .into_any_element(),
            SettingItem::Element { render } => (render)(&options, window, cx),
            SettingItem::ActionButton {
                render,
                on_click,
                visibility_filter,
            } => {
                let visible = visibility_filter.as_ref().map_or(true, |f| f(cx));
                if !visible {
                    return div().into_any_element();
                }
                let element = (render)(&options, window, cx);
                let on_click = on_click.clone();
                div()
                    .id(SharedString::from(format!(
                        "action-button-{}",
                        options.item_ix
                    )))
                    .child(element)
                    .on_click(move |_, window, cx| {
                        on_click.borrow_mut()(window, cx);
                    })
                    .into_any_element()
            }
        }
    }
}
