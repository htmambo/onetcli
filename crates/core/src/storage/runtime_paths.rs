use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// 主题版本文件，记录当前打包的 theme 集校验和。
const THEMES_VERSION_FILE: &str = ".themes_version";

pub fn get_config_dir() -> Result<PathBuf> {
    let config_dir = if cfg!(target_os = "macos") {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".config")
            .join("one-hub")
    } else if cfg!(target_os = "windows") {
        dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
            .join("one-hub")
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".config")
            .join("one-hub")
    };

    Ok(config_dir)
}

pub fn get_db_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("one-hub.db"))
}

pub fn get_download_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

pub fn get_themes_dir() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("themes"))
}

/// 返回当前运行态应使用的主题目录。
pub fn get_runtime_themes_dir() -> Result<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dev_themes_dir) = find_workspace_themes_dir_for_exe(&exe_path) {
            eprintln!(
                "[themes] Dev mode, workspace themes: {}",
                dev_themes_dir.display()
            );
            return Ok(dev_themes_dir);
        }
    }

    let dir = get_themes_dir()?;
    eprintln!(
        "[themes] NOT dev mode, using user dir: {}, exe: {:?}",
        dir.display(),
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .ok()
    );
    Ok(dir)
}

pub fn ensure_themes_copied() -> Result<()> {
    if let Ok(exe_path) = std::env::current_exe() {
        if find_workspace_themes_dir_for_exe(&exe_path).is_some() {
            return Ok(());
        }
    }

    let themes_dir = get_themes_dir()?;
    let user_version_file = themes_dir.join(THEMES_VERSION_FILE);
    let current_version = std::fs::read_to_string(&user_version_file).ok();

    if let Some(bundled) = find_bundled_themes_dir() {
        if bundled.exists() {
            let bundled_version = get_bundled_themes_version(&bundled);

            if current_version == bundled_version {
                return Ok(());
            }

            std::fs::create_dir_all(&themes_dir)?;
            for entry in std::fs::read_dir(bundled)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && is_theme_file(&path) {
                    let file_name = path.file_name().unwrap();
                    let dest = themes_dir.join(file_name);
                    std::fs::copy(&path, &dest)?;
                }
            }

            if let Some(version) = bundled_version {
                std::fs::write(&user_version_file, version)?;
            }
        }
    }

    Ok(())
}

pub fn get_queries_dir() -> Result<PathBuf> {
    let queries_dir = get_config_dir()?.join("queries");
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }
    Ok(queries_dir)
}

fn is_theme_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("json" | "jsonc")
    )
}

fn get_bundled_themes_version(themes_dir: &Path) -> Option<String> {
    let mut entries: Vec<_> = match std::fs::read_dir(themes_dir) {
        Ok(dir) => dir
            .filter_map(|e| e.ok())
            .filter(|e| is_theme_file(&e.path()))
            .collect(),
        Err(_) => return None,
    };
    entries.sort_by_key(|e| e.file_name());

    let mut hasher = DefaultHasher::new();
    for entry in entries {
        entry.file_name().hash(&mut hasher);
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            content.hash(&mut hasher);
        }
    }
    Some(format!("{:x}", hasher.finish()))
}

fn find_bundled_themes_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    #[cfg(target_os = "linux")]
    let installed_path = exe_dir.join("../share/onetcli/themes");

    #[cfg(target_os = "macos")]
    let installed_path = exe_dir.join("../Resources/themes");

    #[cfg(target_os = "windows")]
    let installed_path = exe_dir.join("../share/onetcli/themes");

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let installed_path = exe_dir.join("share/onetcli/themes");

    if let Ok(installed) = installed_path.canonicalize() {
        if installed.exists() {
            return Some(installed);
        }
    }

    None
}

fn find_workspace_themes_dir_for_exe(exe_path: &Path) -> Option<PathBuf> {
    let target_dir = exe_path
        .parent()?
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("target"))?;
    let workspace_dir = target_dir.parent()?;
    let themes_dir = workspace_dir.join("themes");

    if workspace_dir.join("Cargo.toml").is_file() && theme_dir_has_theme_files(&themes_dir) {
        return Some(themes_dir);
    }

    None
}

fn theme_dir_has_theme_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| is_theme_file(&entry.path()))
}
