//! 数据传输向导第三步：对象清单确认与传输选项

use gpui::{App, Context, IntoElement, ParentElement, Styled, div};
use gpui_component::{ActiveTheme, checkbox::Checkbox, h_flex, scroll::ScrollableElement, v_flex};
use rust_i18n::t;

use super::view::DataTransferWindow;

fn summary_cell(text: String) -> impl IntoElement {
    div()
        .flex_1()
        .text_sm()
        .text_ellipsis()
        .overflow_hidden()
        .child(text)
}

fn summary_row(source: String, target: String, mode: String, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .py_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(summary_cell(source))
        .child(summary_cell(target))
        .child(summary_cell(mode))
}

impl DataTransferWindow {
    fn render_summary_list(&self, cx: &App) -> impl IntoElement {
        let header = h_flex()
            .gap_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(summary_cell(t!("DataTransfer.summary_source").to_string()))
            .child(summary_cell(t!("DataTransfer.summary_target").to_string()))
            .child(summary_cell(t!("DataTransfer.summary_mode").to_string()));

        let source_db = self.source.selected_database(cx).unwrap_or_default();
        let target_db = self.target.selected_database(cx).unwrap_or_default();
        let mode_table = t!("DataTransfer.mode_table").to_string();
        let mode_view = t!("DataTransfer.mode_view").to_string();

        let rows = self
            .objects
            .selected_tables_in_order()
            .into_iter()
            .map(|name| {
                (
                    format!("{}.{}", source_db, name),
                    format!("{}.{}", target_db, name),
                    mode_table.clone(),
                )
            })
            .chain(
                self.objects
                    .selected_views_in_order()
                    .into_iter()
                    .map(|name| {
                        (
                            format!("{}.{}", source_db, name),
                            format!("{}.{}", target_db, name),
                            mode_view.clone(),
                        )
                    }),
            )
            .collect::<Vec<_>>();

        div().flex_1().min_h_0().overflow_y_scrollbar().child(
            v_flex().child(header).children(
                rows.into_iter()
                    .map(|(source, target, mode)| summary_row(source, target, mode, cx)),
            ),
        )
    }

    /// 渲染第三步：传输清单 + 选项
    pub(crate) fn render_summary_step(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_3()
            .p_4()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("DataTransfer.summary_hint").to_string()),
            )
            .child(self.render_summary_list(cx))
            .child(
                Checkbox::new("continue-on-error")
                    .checked(self.continue_on_error)
                    .label(t!("DataTransfer.continue_on_error").to_string())
                    .on_click(cx.listener(|view, checked, _window, cx| {
                        view.continue_on_error = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("drop-target-first")
                    .checked(self.drop_target_first)
                    .label(t!("DataTransfer.drop_target_first").to_string())
                    .on_click(cx.listener(|view, checked, _window, cx| {
                        view.drop_target_first = *checked;
                        cx.notify();
                    })),
            )
    }
}
