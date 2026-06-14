//! 设置相关的纯枚举类型：设置页面、数据库打开方式、连接列表排序与视图等。
//!
//! 抽取自 `setting_tab.rs`（轮 6 重构）。这些类型通过父模块以
//! `pub(crate) use` 重导出，维持 `crate::setting_tab::*` 外部
//! 引用路径不变。
//!
//! `select_index` 仅供父模块 `SettingsPanel` 使用，故标 `pub(super)`。

use db_view::LargeTextEditorOpenMode;
use gpui_component::setting::SelectIndex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsPanelPage {
    #[default]
    General,
}

impl SettingsPanelPage {
    pub(super) fn select_index(self) -> SelectIndex {
        let _ = self;
        SelectIndex::default()
    }
}

/// 数据库打开方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DatabaseOpenMode {
    /// 单库模式：每个数据库单独打开一个标签页
    #[default]
    Single,
    /// 工作区模式：按工作区分组打开，同一工作区的数据库在同一标签页
    Workspace,
}

impl DatabaseOpenMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseOpenMode::Single => "single",
            DatabaseOpenMode::Workspace => "workspace",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "workspace" => DatabaseOpenMode::Workspace,
            _ => DatabaseOpenMode::Single,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LargeTextCellEditorOpenMode {
    #[default]
    SidebarPreview,
    Dialog,
}

impl LargeTextCellEditorOpenMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LargeTextCellEditorOpenMode::SidebarPreview => "sidebar_preview",
            LargeTextCellEditorOpenMode::Dialog => "dialog",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "dialog" => LargeTextCellEditorOpenMode::Dialog,
            _ => LargeTextCellEditorOpenMode::SidebarPreview,
        }
    }
}

impl From<LargeTextCellEditorOpenMode> for LargeTextEditorOpenMode {
    fn from(value: LargeTextCellEditorOpenMode) -> Self {
        match value {
            LargeTextCellEditorOpenMode::SidebarPreview => LargeTextEditorOpenMode::SidebarPreview,
            LargeTextCellEditorOpenMode::Dialog => LargeTextEditorOpenMode::Dialog,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListSortField {
    Name,
    CreatedAt,
    Manual,
    #[default]
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListSortOrder {
    Ascending,
    #[default]
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListViewMode {
    #[default]
    Card,
    List,
}
