//! 终端 Agent 模块：注册终端操作员并初始化桥接层。

mod prompt;
mod terminal_operator;
mod tools;

use gpui::{App, BorrowAppContext};
use one_core::agent::registry::AgentRegistry;

pub use terminal_operator::TerminalOperatorAgent;

/// 初始化终端桥接层并注册终端操作员 Agent（幂等）。
pub fn init(cx: &mut App) {
    crate::agent_bridge::init(cx);
    cx.update_global::<AgentRegistry, _>(|registry, _| {
        registry.register(TerminalOperatorAgent::new());
    });
}

#[cfg(test)]
mod tests;
