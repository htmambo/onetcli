//! 向导窗口的固定框架：步骤条与底部按钮区

use std::sync::atomic::Ordering;

use gpui::{App, Context, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder};
use gpui_component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
    stepper::{Stepper, StepperItem},
};
use rust_i18n::t;

use super::view::{DataTransferWindow, TransferStep};

impl DataTransferWindow {
    pub(crate) fn render_stepper(&self, cx: &App) -> impl IntoElement {
        let items = TransferStep::titles()
            .into_iter()
            .map(|title| StepperItem::new().child(title));
        div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Stepper::new("data-transfer-stepper")
                    .selected_index(self.step.index())
                    .disabled(true)
                    .items(items),
            )
    }

    fn render_back_buttons(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        footer: gpui::Div,
    ) -> gpui::Div {
        footer
            .when(self.step != TransferStep::Execute, |this| {
                this.child(
                    Button::new("cancel")
                        .small()
                        .child(t!("Common.cancel").to_string())
                        .on_click(|_, window, _cx| {
                            window.remove_window();
                        }),
                )
            })
            .when(
                matches!(self.step, TransferStep::Objects | TransferStep::Summary),
                |this| {
                    this.child(
                        Button::new("prev")
                            .small()
                            .child(t!("Common.previous").to_string())
                            .on_click(window.listener_for(&cx.entity(), |view, _, _, cx| {
                                view.go_prev(cx);
                            })),
                    )
                },
            )
    }

    fn render_forward_buttons(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        footer: gpui::Div,
    ) -> gpui::Div {
        let can_next = self.can_go_next(cx);
        footer
            .when(
                matches!(self.step, TransferStep::Endpoints | TransferStep::Objects),
                |this| {
                    this.child(
                        Button::new("next")
                            .small()
                            .primary()
                            .disabled(!can_next)
                            .child(t!("Common.next").to_string())
                            .on_click(window.listener_for(&cx.entity(), |view, _, _, cx| {
                                view.go_next(cx);
                            })),
                    )
                },
            )
            .when(self.step == TransferStep::Summary, |this| {
                this.child(
                    Button::new("start")
                        .small()
                        .primary()
                        .child(t!("DataTransfer.start_transfer").to_string())
                        .on_click(window.listener_for(&cx.entity(), |view, _, _, cx| {
                            view.go_next(cx);
                        })),
                )
            })
    }

    fn render_exec_buttons(&self, cx: &mut Context<Self>, footer: gpui::Div) -> gpui::Div {
        let running = self.exec.running.load(Ordering::Relaxed);
        let finished = *self.exec.finished.read(cx);
        let cancel_requested = self.exec.cancel.load(Ordering::Relaxed);
        footer
            .when(running && !cancel_requested, |this| {
                this.child(
                    Button::new("cancel-transfer")
                        .small()
                        .child(t!("Common.cancel").to_string())
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.exec.cancel.store(true, Ordering::Relaxed);
                            cx.notify();
                        })),
                )
            })
            .when(running && cancel_requested, |this| {
                this.child(
                    Button::new("cancelling")
                        .small()
                        .loading(true)
                        .child(t!("DataTransfer.cancelling").to_string()),
                )
            })
            .when(finished, |this| {
                this.child(
                    Button::new("close")
                        .small()
                        .primary()
                        .child(t!("Common.finish").to_string())
                        .on_click(|_, window, _cx| {
                            window.remove_window();
                        }),
                )
            })
    }

    pub(crate) fn render_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let footer = h_flex()
            .justify_end()
            .gap_2()
            .p_4()
            .border_t_1()
            .border_color(cx.theme().border)
            .rounded_bl(cx.theme().radius_lg)
            .rounded_br(cx.theme().radius_lg);
        let footer = self.render_back_buttons(window, cx, footer);
        let footer = self.render_forward_buttons(window, cx, footer);
        let footer = if self.step == TransferStep::Execute {
            self.render_exec_buttons(cx, footer)
        } else {
            footer
        };
        footer
    }
}
