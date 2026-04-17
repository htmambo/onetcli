//! 动效 Token
//!
//! 统一管理动画时长和缓动函数。

use std::time::Duration;

/// 动效 token 结构体
#[derive(Debug, Clone, Copy)]
pub struct MotionTokens {
    /// 即时反馈 — 50ms
    pub duration_instant: Duration,
    /// 快速过渡 — 150ms
    pub duration_fast: Duration,
    /// 正常过渡 — 250ms
    pub duration_normal: Duration,
    /// 慢速过渡 — 400ms
    pub duration_slow: Duration,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            duration_instant: Duration::from_millis(50),
            duration_fast: Duration::from_millis(150),
            duration_normal: Duration::from_millis(250),
            duration_slow: Duration::from_millis(400),
        }
    }
}

/// 缓动函数类型
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    /// 默认 — ease-in-out
    Default,
    /// 减速曲线 — 缓入
    Decelerate,
    /// 加速曲线 — 缓出
    Accelerate,
}

impl Easing {
    /// 返回 cubic-bezier 参数 (x1, y1, x2, y2)
    pub fn bezier(self) -> (f64, f64, f64, f64) {
        match self {
            Self::Default => (0.4, 0.0, 0.2, 1.0),
            Self::Decelerate => (0.0, 0.0, 0.2, 1.0),
            Self::Accelerate => (0.4, 0.0, 1.0, 1.0),
        }
    }
}
