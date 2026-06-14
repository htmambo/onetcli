//! 设置页相关全局用户状态。
//!
//! 抽取自 `setting_tab.rs`（轮 2 重构）。`GlobalCurrentUser` 通过父模块
//! `pub(crate) use` 重导出，维持 `crate::setting_tab::GlobalCurrentUser`
//! 外部引用路径不变；`PendingSettingsPanelPage` 仅供父模块使用，
//! 标 `pub(super)`。

use std::sync::{Arc, RwLock};

use gpui::{App, Global};
use one_core::cloud_sync::{GlobalCloudUser, UserInfo};

use super::SettingsPanelPage;

/// 全局当前用户状态
///
/// 用于在设置面板中显示用户信息和执行登出操作。
#[derive(Clone, Default)]
pub struct GlobalCurrentUser {
    user: Arc<RwLock<Option<UserInfo>>>,
}

impl Global for GlobalCurrentUser {}

impl GlobalCurrentUser {
    /// 获取当前用户
    pub fn get_user(cx: &App) -> Option<UserInfo> {
        if let Some(state) = cx.try_global::<GlobalCurrentUser>() {
            state.user.read().ok().and_then(|u| u.clone())
        } else {
            None
        }
    }

    /// 设置当前用户
    pub fn set_user(user: Option<UserInfo>, cx: &mut App) {
        // 重新创建 GlobalCurrentUser 以触发 observe_global 回调，使 SettingsPanel 能刷新 UI
        cx.set_global(Self {
            user: Arc::new(RwLock::new(user.clone())),
        });
        GlobalCloudUser::set_user(user, cx);
    }
}

#[derive(Clone, Default)]
pub(super) struct PendingSettingsPanelPage {
    page: Arc<RwLock<Option<SettingsPanelPage>>>,
}

impl Global for PendingSettingsPanelPage {}

impl PendingSettingsPanelPage {
    pub(super) fn take(cx: &mut App) -> Option<SettingsPanelPage> {
        if let Some(state) = cx.try_global::<PendingSettingsPanelPage>() {
            if let Ok(mut guard) = state.page.write() {
                return guard.take();
            }
        }
        None
    }
}
