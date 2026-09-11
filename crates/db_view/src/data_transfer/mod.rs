//! 数据传输向导 popup：端点选择 -> 对象选择 -> 摘要确认 -> 执行进度

mod chrome;
mod progress;
mod step_endpoints;
mod step_execute;
mod step_objects;
mod step_summary;
mod view;

pub use view::{DataTransferWindow, TransferStep, open_data_transfer_window};
