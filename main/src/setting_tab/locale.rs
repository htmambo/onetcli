//! 界面语言解析：支持跟随系统。

pub const LOCALE_SYSTEM: &str = "system";
pub const LOCALE_EN: &str = "en";
pub const LOCALE_ZH_CN: &str = "zh-CN";
pub const LOCALE_ZH_HK: &str = "zh-HK";

/// 将设置值解析为实际生效的 locale。
pub fn effective_locale_for_setting(setting: &str) -> &'static str {
    resolve_locale_setting(setting, sys_locale::get_locale().as_deref())
}

pub fn resolve_locale_setting(setting: &str, system_locale: Option<&str>) -> &'static str {
    match setting {
        LOCALE_EN => LOCALE_EN,
        LOCALE_ZH_CN => LOCALE_ZH_CN,
        LOCALE_ZH_HK => LOCALE_ZH_HK,
        LOCALE_SYSTEM | "" => supported_locale_from_system_locale(system_locale.unwrap_or_default())
            .unwrap_or(LOCALE_EN),
        _ => LOCALE_EN,
    }
}

pub fn supported_locale_from_system_locale(system_locale: &str) -> Option<&'static str> {
    let normalized = normalize_locale(system_locale);
    if normalized.is_empty() {
        return None;
    }

    if normalized == "en" || normalized.starts_with("en-") {
        return Some(LOCALE_EN);
    }

    if normalized == "zh" || normalized.starts_with("zh-") {
        if is_traditional_chinese_locale(&normalized) {
            return Some(LOCALE_ZH_HK);
        }
        return Some(LOCALE_ZH_CN);
    }

    None
}

fn normalize_locale(locale: &str) -> String {
    locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .trim()
        .replace('_', "-")
        .to_ascii_lowercase()
}

fn is_traditional_chinese_locale(normalized_locale: &str) -> bool {
    normalized_locale
        .split('-')
        .any(|part| matches!(part, "hant" | "hk" | "tw" | "mo" | "cht" | "traditional"))
}

#[cfg(test)]
mod tests {
    use super::{LOCALE_SYSTEM, resolve_locale_setting, supported_locale_from_system_locale};

    #[test]
    fn supported_locale_from_system_locale_maps_cross_platform_values() {
        assert_eq!(
            Some("zh-CN"),
            supported_locale_from_system_locale("zh-Hans-CN")
        );
        assert_eq!(
            Some("zh-CN"),
            supported_locale_from_system_locale("zh_CN.UTF-8")
        );
        assert_eq!(
            Some("zh-HK"),
            supported_locale_from_system_locale("zh-Hant-HK")
        );
        assert_eq!(Some("zh-HK"), supported_locale_from_system_locale("zh_TW"));
        assert_eq!(Some("en"), supported_locale_from_system_locale("en-US"));
        assert_eq!(None, supported_locale_from_system_locale("fr-FR"));
    }

    #[test]
    fn resolve_locale_setting_uses_system_fallback_for_system_mode() {
        assert_eq!("en", resolve_locale_setting(LOCALE_SYSTEM, None));
        assert_eq!(
            "zh-HK",
            resolve_locale_setting(LOCALE_SYSTEM, Some("zh-Hant-TW"))
        );
        assert_eq!("en", resolve_locale_setting("en", Some("zh-CN")));
        assert_eq!(
            "zh-CN",
            resolve_locale_setting("", Some("zh_CN.UTF-8"))
        );
    }
}
