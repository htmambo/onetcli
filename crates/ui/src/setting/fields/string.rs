use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Entity, IntoElement, SharedString, StyleRefinement, Styled,
    Window, prelude::FluentBuilder as _,
};

use crate::{
    AxisExt as _, Sizable, StyledExt,
    input::{Input, InputEvent, InputState},
    setting::{
        AnySettingField, RenderOptions,
        fields::{SettingFieldRender, get_value, set_value},
    },
};

pub(crate) struct StringField<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> StringField<T> {
    pub(crate) fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

struct State {
    input: Entity<InputState>,
    initial_value: SharedString,
    _subscription: gpui::Subscription,
}

impl<T> SettingFieldRender for StringField<T>
where
    T: Into<SharedString> + From<SharedString> + Clone + 'static,
{
    fn render(
        &self,
        field: Rc<dyn AnySettingField>,
        options: &RenderOptions,
        style: &StyleRefinement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let value = get_value::<T>(&field, cx);
        let set_value = set_value::<T>(&field, cx);
        let current_value: SharedString = value.clone().into();

        let state = window.use_keyed_state(
            SharedString::from(format!(
                "string-state-{}-{}-{}",
                options.page_ix, options.group_ix, options.item_ix
            )),
            cx,
            |window, cx| {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
                let _subscription = cx.subscribe(&input, {
                    move |state: &mut State, input, event: &InputEvent, cx| match event {
                        InputEvent::Change => {
                            let value = input.read(cx).value();
                            if value == state.initial_value {
                                return;
                            }
                            state.initial_value = value.clone();
                            set_value(value.into(), cx);
                        }
                        _ => {}
                    }
                });

                State {
                    input,
                    initial_value: current_value.clone(),
                    _subscription,
                }
            },
        );

        let cached_input_value = state.read(cx).input.read(cx).value();
        let should_sync = {
            let state = state.read(cx);
            state.initial_value != current_value || cached_input_value != current_value
        };
        if should_sync {
            let next_value = current_value.clone();
            state.update(cx, |state, cx| {
                state.initial_value = next_value.clone();
                state.input.update(cx, |input, cx| {
                    input.set_value(next_value.clone(), window, cx);
                });
            });
        }

        let state = state.read(cx);

        Input::new(&state.input)
            .with_size(options.size)
            .map(|this| {
                if options.layout.is_horizontal() {
                    this.w_64()
                } else {
                    this.w_full()
                }
            })
            .refine_style(style)
            .into_any_element()
    }
}
