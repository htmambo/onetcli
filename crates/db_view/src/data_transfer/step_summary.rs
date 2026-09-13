//! 数据传输向导第三步：对象清单确认与传输选项

use db::PrecheckSeverity;
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

/// 预检条目严重级别对应的显示颜色
fn severity_color(severity: &PrecheckSeverity, cx: &App) -> gpui::Hsla {
    match severity {
        PrecheckSeverity::Error => cx.theme().danger,
        PrecheckSeverity::Warning => cx.theme().warning,
        PrecheckSeverity::Info => cx.theme().muted_foreground,
    }
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

        // Scrollable 包装器只继承 size，flex_1/min_h_0 须加在外层普通容器上
        div().flex_1().min_h_0().child(
            div().size_full().overflow_y_scrollbar().child(
                v_flex().child(header).children(
                    rows.into_iter()
                        .map(|(source, target, mode)| summary_row(source, target, mode, cx)),
                ),
            ),
        )
    }

    /// 渲染预检测区块：标题、两端版本、逐条问题与阻塞提示
    fn render_precheck_section(&self, cx: &App) -> impl IntoElement {
        let mut section = v_flex()
            .gap_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .pt_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(t!("DataTransfer.precheck_section_title").to_string()),
            );

        if *self.precheck_loading.read(cx) {
            return section.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("DataTransfer.precheck_running").to_string()),
            );
        }

        let Some(report) = self.precheck_report.read(cx).as_ref() else {
            return section;
        };

        if let (Some(source), Some(target)) = (&report.source_version, &report.target_version) {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        t!(
                            "DataTransfer.precheck_versions_label",
                            source = source.clone(),
                            target = target.clone()
                        )
                        .to_string(),
                    ),
            );
        }

        for issue in &report.issues {
            let color = severity_color(&issue.severity, cx);
            section = section.child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(color)
                            .child(issue.title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(issue.detail.clone()),
                    ),
            );
        }

        if report.has_blocking_errors() {
            section = section.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(t!("DataTransfer.precheck_blocking_hint").to_string()),
            );
        }
        section
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
            .child(self.render_precheck_section(cx))
    }
}
