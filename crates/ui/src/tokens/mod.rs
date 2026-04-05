//! Design Token 系统 — 原子化设计决策存储
//!
//! 架构分层：
//! - `color/` — 颜色 token（Primitives + Semantic）
//! - `spacing.rs` — 间距 token
//! - `typography.rs` — 字体 token
//! - `radius.rs` — 圆角 token
//! - `shadow.rs` — 阴影 token
//! - `motion.rs` — 动效 token

pub mod color;
pub mod spacing;
pub mod typography;
pub mod radius;
pub mod shadow;
pub mod motion;

pub use color::semantic::SemanticColors;
pub use spacing::Spacing;
pub use typography::TypographyTokens;
pub use radius::Radius;
pub use shadow::ShadowToken;
pub use motion::MotionTokens;
