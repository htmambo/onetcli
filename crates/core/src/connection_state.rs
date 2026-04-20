//! 连接状态管理模块
//!
//! 提供 `ConnectionState` 枚举和 `set_connection_active` 辅助函数，
//! 用于统一 SSH/SFTP/串口终端的连接状态管理。
//!
//! 此模块从 `storage::models` 中提取，因为 `ConnectionState` 和
//! `set_connection_active` 属于运行时/UI 关注点，而非存储/持久化。

use gpui::{Context, Global};
use std::collections::HashSet;

/// 连接状态
///
/// 统一定义 SSH/SFTP/串口终端连接的状态枚举，
/// 消除 `terminal` 和 `sftp_view` 中的重复定义。
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected { error: Option<String> },
}

/// 活跃连接状态 - 用于跟踪哪些连接当前已打开
#[derive(Default)]
pub struct ActiveConnections {
    active_ids: HashSet<i64>,
}

impl Global for ActiveConnections {}

impl ActiveConnections {
    pub fn new() -> Self {
        Self {
            active_ids: HashSet::new(),
        }
    }

    pub fn add(&mut self, conn_id: i64) {
        self.active_ids.insert(conn_id);
    }

    pub fn remove(&mut self, conn_id: i64) {
        self.active_ids.remove(&conn_id);
    }

    pub fn is_active(&self, conn_id: i64) -> bool {
        self.active_ids.contains(&conn_id)
    }

    pub fn active_count(&self) -> usize {
        self.active_ids.len()
    }
}

/// 更新活跃连接注册表。
///
/// 统一 `terminal` 和 `sftp_view` 中重复的 `set_connection_active` 实现。
pub fn set_connection_active<T>(connection_id: Option<i64>, active: bool, cx: &mut Context<T>)
where
    T: 'static,
{
    let Some(connection_id) = connection_id else {
        return;
    };
    let global_state = cx.global_mut::<ActiveConnections>();
    if active {
        global_state.add(connection_id);
    } else {
        global_state.remove(connection_id);
    }
}
