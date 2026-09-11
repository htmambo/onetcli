//! 数据传输引擎：把源连接库中的表（结构+数据）与视图复制到目标连接的库

mod engine;
mod session;
mod sql;
mod types;
mod views;

#[cfg(test)]
mod tests;

pub use engine::{DEFAULT_BATCH_SIZE, run_transfer};
pub use sql::{
    build_create_table_sql, build_create_view_sql, build_insert_sql, format_insert_value,
};
pub use types::{TransferConfig, TransferProgressEvent, TransferProgressSender, TransferSummary};
