use crate::setting_tab::AppSettings;
use one_core::storage::get_config_dir;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// 初始化 tracing，并尽量把日志写入用户配置目录。
pub fn init_tracing(settings: &AppSettings) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match configured_log_file_path(&settings.log_file_path) {
        Ok(log_file_path) => match log_file_appender(&log_file_path) {
            Ok(file_appender) => {
                let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
                Box::leak(Box::new(guard));
                tracing_subscriber::registry()
                    .with(tracing_subscriber::fmt::layer())
                    .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
                    .with(env_filter)
                    .init();
            }
            Err(err) => {
                tracing_subscriber::registry()
                    .with(tracing_subscriber::fmt::layer())
                    .with(env_filter)
                    .init();
                tracing::error!(path = %log_file_path.display(), error = %err, "日志文件初始化失败");
            }
        },
        Err(err) => {
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer())
                .with(env_filter)
                .init();
            tracing::error!(error = %err, "默认日志目录初始化失败");
        }
    }
}

pub fn configured_log_file_path(value: &str) -> anyhow::Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default_log_file_path()
    } else {
        Ok(PathBuf::from(trimmed))
    }
}

pub fn default_log_file_path() -> anyhow::Result<PathBuf> {
    Ok(get_config_dir()?.join("logs").join("omnihub.log"))
}

pub fn log_file_appender(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::{configured_log_file_path, default_log_file_path, log_file_appender};
    use std::io::Write;

    #[test]
    fn configured_log_file_path_uses_default_for_empty_value() {
        let default_path = default_log_file_path().expect("应返回默认日志路径");

        assert_eq!(configured_log_file_path("").unwrap(), default_path);
        assert_eq!(configured_log_file_path("   ").unwrap(), default_path);
    }

    #[test]
    fn configured_log_file_path_trims_value() {
        let path = configured_log_file_path("  /tmp/omnihub.log  ").expect("应返回日志路径");
        assert_eq!(path, std::path::PathBuf::from("/tmp/omnihub.log"));
    }

    #[test]
    fn log_file_appender_creates_parent_directories_and_appends() {
        let path = std::env::temp_dir()
            .join(format!("omnihub-log-test-{}", std::process::id()))
            .join("nested")
            .join("app.log");

        {
            let mut file = log_file_appender(&path).expect("应创建日志文件");
            writeln!(file, "first").expect("应写入第一行");
        }
        {
            let mut file = log_file_appender(&path).expect("应重新打开日志文件");
            writeln!(file, "second").expect("应追加第二行");
        }

        let content = std::fs::read_to_string(&path).expect("应读取日志文件");
        assert_eq!(content, "first\nsecond\n");

        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn log_file_appender_creates_private_file() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir()
            .join(format!(
                "omnihub-log-permission-test-{}",
                std::process::id()
            ))
            .join("app.log");
        let _file = log_file_appender(&path).expect("应创建日志文件");

        let mode = std::fs::metadata(&path)
            .expect("应读取日志文件元数据")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
