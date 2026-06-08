//! Popup 圆角半径的 UI 层使用说明。
//!
//! 单一真相源位于 `one_core::popup_window::ROUNDED_POPUP_RADIUS_PX`，
//! 定义在 core crate 内部以避免 one-ui 反向依赖 one-core。
//!
//! 下游消费者（如 `main`）直接导入 `one_core::popup_window::ROUNDED_POPUP_RADIUS_PX`
//! 或 `one_core::popup_window::popup_radius_gpui`。
//!
//! 本模块提供 UI 层视角的常量镜像与文档说明，方便未来扩展；
//! 若 one-ui 后续需要独立于 one-core 的 UI 圆角默认值，可在本文件加常量。
//!
//! 详见 `AGENTS.md:329` 已验证经验：Wayland 下 popup_window 圆角背景外溢时，
//! 应让内容层自绘圆角，壳层只负责边框/阴影。

/// UI 层约定的 popup 圆角基线（像素），与 `one_core::popup_window::ROUNDED_POPUP_RADIUS_PX` 保持一致。
/// 此处独立定义以便 one-ui 内部使用，避免反向依赖 one-core。
pub const ROUNDED_POPUP_RADIUS_PX: f32 = 8.0;

/// 编译期断言：保证 `ROUNDED_POPUP_RADIUS_PX` 与 one-core 常量语义一致。
/// 若修改此处必须同步 `crates/core/src/popup_window.rs`。
const _: () = assert!(
    ROUNDED_POPUP_RADIUS_PX == 8.0,
    "one_ui::popup_helpers::ROUNDED_POPUP_RADIUS_PX 必须与 one_core 保持一致"
);
