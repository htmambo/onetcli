//! 设置页 - 关于 区块（轮 9a 重构抽取）。
//!
//! 仅供父模块 `SettingsPanel::render` 使用，标 `pub(super)`。

use gpui::{App, ClickEvent, FontWeight, IntoElement, ParentElement, Styled};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::clipboard::Clipboard;
use gpui_component::{ActiveTheme, IconName, Sizable, h_flex, v_flex};
use rust_i18n::t;

const GITHUB_URL: &str = "https://github.com/htmambo/onetcli";

/// 渲染关于页面
pub(super) fn render_about_section(cx: &App) -> gpui::AnyElement {
    let version = env!("CARGO_PKG_VERSION");
    let muted = cx.theme().muted_foreground;

    let disclaimer_items: Vec<String> = (1..=5)
        .map(|i| {
            let key = format!("Settings.About.disclaimer_item_{}", i);
            let text = t!(&key).to_string();
            format!("{}. {}", i, text)
        })
        .collect();

    let data_safety_items: Vec<String> = (1..=3)
        .map(|i| {
            let key = format!("Settings.About.data_safety_item_{}", i);
            let text = t!(&key).to_string();
            format!("• {}", text)
        })
        .collect();

    v_flex()
        .gap_4()
        .p_4()
        // 版本信息
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(gpui::div().text_sm().child(format!(
                    "{}: {}",
                    t!("Settings.About.version"),
                    version
                ))),
        )
        // GitHub 开源地址
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    gpui::div()
                        .text_sm()
                        .child(format!("{}: ", t!("Settings.About.opensource_label"))),
                )
                .child(
                    gpui::div()
                        .text_sm()
                        .text_color(cx.theme().link)
                        .child(GITHUB_URL),
                )
                .child(Clipboard::new("about-copy-github-url").value(GITHUB_URL))
                .child(
                    Button::new("about-open-github")
                        .icon(IconName::ExternalLink)
                        .xsmall()
                        .ghost()
                        .on_click(|_: &ClickEvent, _, cx| {
                            cx.open_url(GITHUB_URL);
                        }),
                ),
        )
        // 免责声明
        .child(
            v_flex()
                .gap_2()
                .child(
                    gpui::div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t!("Settings.About.disclaimer_title").to_string()),
                )
                .child(
                    gpui::div()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("Settings.About.disclaimer_status").to_string()),
                )
                .child(
                    v_flex().gap_1().pl_2().children(
                        disclaimer_items
                            .into_iter()
                            .map(|item| gpui::div().text_sm().text_color(muted).child(item)),
                    ),
                ),
        )
        // 数据与安全提示
        .child(
            v_flex()
                .gap_2()
                .child(
                    gpui::div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t!("Settings.About.data_safety_title").to_string()),
                )
                .child(
                    v_flex().gap_1().pl_2().children(
                        data_safety_items
                            .into_iter()
                            .map(|item| gpui::div().text_sm().text_color(muted).child(item)),
                    ),
                ),
        )
        .into_any_element()
}
