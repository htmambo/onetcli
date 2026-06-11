pub mod connection;
pub mod demo_database;
pub mod manager;
pub mod migration;
pub mod models;
pub mod quick_command;
pub mod repository;
pub mod row_mapping;
pub mod runtime_paths;
pub mod sftp_favorite_path;
pub mod ssh_convert;
pub mod traits;

use gpui::App;
pub use manager::*;
pub use models::*;
pub use quick_command::*;
pub use repository::*;
pub use runtime_paths::*;
pub use sftp_favorite_path::*;

// 从 connection_state 重新导出，确保全局状态类型唯一
pub use crate::connection_state::ActiveConnections;

pub fn init(cx: &mut App) {
    cx.set_global(ActiveConnections::new());
    manager::init(cx);
    repository::init(cx);

    // 首次启动时创建演示数据库
    let storage = cx.global::<GlobalStorageState>().storage.clone();
    if let Some(conn_repo) = storage.get::<ConnectionRepository>() {
        demo_database::try_init_demo(&conn_repo);
    }
}
