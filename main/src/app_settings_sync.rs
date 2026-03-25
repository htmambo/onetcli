use crate::setting_tab::{APP_SETTINGS_SYNC_ITEM_NAME, AppSettings, DatabaseOpenMode};
use one_core::cloud_sync::{
    CloudSyncData, CloudSyncService, SyncEngine, SyncError, SyncTypeHandler, SyncableItem, Team,
    data_type,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppSettingsPlainData {
    locale: String,
    theme_mode: String,
    auto_switch_theme: bool,
    font_family: String,
    font_size: f64,
    terminal_font_size: f64,
    terminal_auto_copy: bool,
    terminal_middle_click_paste: bool,
    terminal_sync_path_with_terminal: bool,
    terminal_theme: String,
    terminal_cursor_blink: bool,
    terminal_confirm_multiline_paste: bool,
    terminal_confirm_high_risk_command: bool,
    auto_update: bool,
    database_open_mode: DatabaseOpenMode,
    enable_sql_auto_save: bool,
    sql_auto_save_interval: f64,
}

impl From<&AppSettings> for AppSettingsPlainData {
    fn from(settings: &AppSettings) -> Self {
        Self {
            locale: settings.locale.clone(),
            theme_mode: settings.theme_mode.clone(),
            auto_switch_theme: settings.auto_switch_theme,
            font_family: settings.font_family.clone(),
            font_size: settings.font_size,
            terminal_font_size: settings.terminal_font_size,
            terminal_auto_copy: settings.terminal_auto_copy,
            terminal_middle_click_paste: settings.terminal_middle_click_paste,
            terminal_sync_path_with_terminal: settings.terminal_sync_path_with_terminal,
            terminal_theme: settings.terminal_theme.clone(),
            terminal_cursor_blink: settings.terminal_cursor_blink,
            terminal_confirm_multiline_paste: settings.terminal_confirm_multiline_paste,
            terminal_confirm_high_risk_command: settings.terminal_confirm_high_risk_command,
            auto_update: settings.auto_update,
            database_open_mode: settings.database_open_mode,
            enable_sql_auto_save: settings.enable_sql_auto_save,
            sql_auto_save_interval: settings.sql_auto_save_interval,
        }
    }
}

impl AppSettingsPlainData {
    fn into_settings(self, cloud_data: &CloudSyncData) -> AppSettings {
        let defaults = AppSettings::default();
        AppSettings {
            locale: self.locale,
            theme_mode: self.theme_mode,
            auto_switch_theme: self.auto_switch_theme,
            font_family: self.font_family,
            font_size: self.font_size,
            terminal_font_size: self.terminal_font_size,
            terminal_auto_copy: self.terminal_auto_copy,
            terminal_middle_click_paste: self.terminal_middle_click_paste,
            terminal_sync_path_with_terminal: self.terminal_sync_path_with_terminal,
            terminal_theme: self.terminal_theme,
            terminal_cursor_blink: self.terminal_cursor_blink,
            terminal_confirm_multiline_paste: self.terminal_confirm_multiline_paste,
            terminal_confirm_high_risk_command: self.terminal_confirm_high_risk_command,
            auto_update: self.auto_update,
            sync_server_url: defaults.sync_server_url,
            database_open_mode: self.database_open_mode,
            enable_sql_auto_save: self.enable_sql_auto_save,
            sql_auto_save_interval: self.sql_auto_save_interval,
            local_id: defaults.local_id,
            remote_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
            updated_at: Some(cloud_data.updated_at / 1000),
        }
    }
}

fn upsert_local_app_settings(item: &AppSettings) {
    let mut settings = AppSettings::load();
    settings.apply_synced_settings(item);
    settings.persist_sync_state();
}

pub struct AppSettingsSyncType;

impl SyncableItem for AppSettings {
    fn local_id(&self) -> Option<i64> {
        Some(self.local_id)
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        self.local_id = id.unwrap_or(self.local_id.max(1));
    }

    fn item_name(&self) -> &str {
        APP_SETTINGS_SYNC_ITEM_NAME
    }

    fn cloud_id(&self) -> Option<&str> {
        self.remote_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.remote_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }
}

impl SyncTypeHandler for AppSettingsSyncType {
    type Item = AppSettings;

    fn data_type(&self) -> &'static str {
        data_type::APP_SETTINGS
    }

    fn display_name(&self) -> &'static str {
        APP_SETTINGS_SYNC_ITEM_NAME
    }

    fn queue_key(&self) -> &'static str {
        data_type::APP_SETTINGS
    }

    fn list_local(&self, _engine: &SyncEngine) -> Result<Vec<Self::Item>, SyncError> {
        Ok(vec![AppSettings::load()])
    }

    fn insert_local(&self, _engine: &SyncEngine, item: &mut Self::Item) -> Result<(), SyncError> {
        upsert_local_app_settings(item);
        Ok(())
    }

    fn update_local_item(&self, _engine: &SyncEngine, item: &Self::Item) -> Result<(), SyncError> {
        upsert_local_app_settings(item);
        Ok(())
    }

    fn delete_local(&self, _engine: &SyncEngine, _id: i64) -> Result<(), SyncError> {
        let mut settings = AppSettings::load();
        settings.remote_id = None;
        settings.last_synced_at = None;
        settings.save();
        Ok(())
    }

    fn on_uploaded(
        &self,
        _engine: &SyncEngine,
        _local_id: i64,
        cloud_id: &str,
    ) -> Result<(), SyncError> {
        let mut settings = AppSettings::load();
        settings.remote_id = Some(cloud_id.to_string());
        settings.last_synced_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0),
        );
        settings.persist_sync_state();
        Ok(())
    }

    fn decrypt_name(&self, _service: &CloudSyncService, data: &CloudSyncData) -> Option<String> {
        if data.name.trim().is_empty() {
            Some(APP_SETTINGS_SYNC_ITEM_NAME.to_string())
        } else {
            Some(data.name.clone())
        }
    }

    fn decrypt(
        &self,
        service: &CloudSyncService,
        data: &CloudSyncData,
    ) -> Result<Self::Item, SyncError> {
        let plain: AppSettingsPlainData = service.decrypt_generic_sync_data(data)?;
        Ok(plain.into_settings(data))
    }

    fn encrypt(
        &self,
        service: &CloudSyncService,
        item: &Self::Item,
        teams: &[Team],
    ) -> Result<CloudSyncData, SyncError> {
        let plain = AppSettingsPlainData::from(item);
        service.prepare_generic_sync_data_upload(
            data_type::APP_SETTINGS,
            APP_SETTINGS_SYNC_ITEM_NAME,
            &plain,
            None,
            teams,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 应用设置同步合并时保留本地同步地址() {
        let mut local = AppSettings {
            sync_server_url: "https://local-sync.example.com".to_string(),
            remote_id: Some("local-remote".to_string()),
            updated_at: Some(10),
            ..AppSettings::default()
        };
        let incoming = AppSettings {
            locale: "en".to_string(),
            theme_mode: "dark".to_string(),
            remote_id: Some("cloud-remote".to_string()),
            updated_at: Some(20),
            ..AppSettings::default()
        };

        local.apply_synced_settings(&incoming);

        assert_eq!(local.sync_server_url, "https://local-sync.example.com");
        assert_eq!(local.locale, "en");
        assert_eq!(local.theme_mode, "dark");
        assert_eq!(local.remote_id.as_deref(), Some("cloud-remote"));
        assert_eq!(local.updated_at, Some(20));
    }
}
