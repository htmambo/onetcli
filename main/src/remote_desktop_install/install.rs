use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, anyhow};

use remote_desktop::PROVIDER_MANIFEST_FILE;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 用户级远程桌面插件目录，必须与 remote_desktop crate 的默认目录保持一致。
pub(crate) fn default_providers_root() -> anyhow::Result<PathBuf> {
    Ok(one_core::storage::get_config_dir()?
        .join("extensions")
        .join("remote_desktop_providers"))
}

/// 从 tar.gz 包安装远程桌面插件，返回安装目录。
/// 失败时自动回滚已存在的旧版本，保证不会出现半安装状态。
pub(crate) fn install_provider_from_archive(
    archive: &Path,
    providers_root: &Path,
) -> anyhow::Result<PathBuf> {
    let staging = make_staging_dir()?;
    let result = install_from_staging(archive, &staging, providers_root);
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn install_from_staging(
    archive: &Path,
    staging: &Path,
    providers_root: &Path,
) -> anyhow::Result<PathBuf> {
    extract_archive(archive, staging)?;
    let package_root = locate_package_root(staging)?;
    let provider_id = read_provider_id(&package_root)?;
    std::fs::create_dir_all(providers_root)
        .with_context(|| format!("创建插件目录失败: {}", providers_root.display()))?;

    let target = providers_root.join(&provider_id);
    let backup = backup_existing_target(providers_root, &provider_id, &target)?;
    if let Err(error) = copy_dir_recursive(&package_root, &target) {
        restore_failed_install(&target, backup.as_deref())?;
        return Err(error);
    }
    match remote_desktop::RemoteDesktopProviderRegistry::load_provider_from_dir(&target) {
        Ok(Some(_)) => {
            remove_install_backup(backup.as_deref());
            Ok(target)
        }
        Ok(None) => {
            restore_failed_install(&target, backup.as_deref())?;
            Err(anyhow!("远程桌面插件清单缺失: {}", target.display()))
        }
        Err(error) => {
            restore_failed_install(&target, backup.as_deref())?;
            Err(error)
        }
    }
}

fn extract_archive(archive: &Path, staging: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("打开插件包失败: {}", archive.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries().context("读取 tar 条目失败")? {
        let mut entry = entry.context("读取 tar 条目失败")?;
        validate_tar_entry(&entry)?;
        let unpacked = entry
            .unpack_in(staging)
            .with_context(|| format!("解包插件包失败: {}", staging.display()))?;
        if !unpacked {
            anyhow::bail!("tar 条目解包目标超出目录");
        }
    }
    Ok(())
}

fn validate_tar_entry<R: std::io::Read>(entry: &tar::Entry<'_, R>) -> anyhow::Result<()> {
    let path = entry.path().context("读取 tar 条目路径失败")?;
    if path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        anyhow::bail!("tar 条目路径越界: {}", path.display());
    }
    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        anyhow::bail!("tar 条目不允许符号链接或硬链接: {}", path.display());
    }
    Ok(())
}

fn locate_package_root(staging: &Path) -> anyhow::Result<PathBuf> {
    if staging.join(PROVIDER_MANIFEST_FILE).exists() {
        return Ok(staging.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(staging)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || is_ignored_name(&entry.file_name()) {
            continue;
        }
        if entry.path().join(PROVIDER_MANIFEST_FILE).exists() {
            candidates.push(entry.path());
        }
    }
    match candidates.as_slice() {
        [dir] => Ok(dir.clone()),
        [] => Err(anyhow!("扩展包缺少 {PROVIDER_MANIFEST_FILE}")),
        _ => Err(anyhow!("扩展包包含多个远程桌面插件目录")),
    }
}

fn is_ignored_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name == ".DS_Store" || name == "__MACOSX" || name.starts_with("._")
}

fn read_provider_id(package_root: &Path) -> anyhow::Result<String> {
    let manifest_path = package_root.join(PROVIDER_MANIFEST_FILE);
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("读取插件清单失败: {}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("解析插件清单失败: {}", manifest_path.display()))?;
    let id = manifest
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("远程桌面插件清单缺少 id 字段"))?;
    if id == "." || id == ".." || id.contains('/') || id.contains('\\') {
        anyhow::bail!("远程桌面插件 id 非法: {id}");
    }
    Ok(id.to_string())
}

fn backup_existing_target(
    providers_root: &Path,
    provider_id: &str,
    target: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    if !target.exists() {
        return Ok(None);
    }
    let backup = make_backup_dir(providers_root, provider_id);
    std::fs::rename(target, &backup)
        .with_context(|| format!("备份已有插件失败: {}", target.display()))?;
    Ok(Some(backup))
}

fn restore_failed_install(target: &Path, backup: Option<&Path>) -> anyhow::Result<()> {
    let _ = std::fs::remove_dir_all(target);
    if let Some(backup) = backup {
        std::fs::rename(backup, target)
            .with_context(|| format!("恢复旧版插件失败: {}", target.display()))?;
    }
    Ok(())
}

fn remove_install_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        if let Err(error) = std::fs::remove_dir_all(backup) {
            tracing::warn!("删除插件安装备份失败 {}: {error:?}", backup.display());
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("创建目录失败: {}", dst.display()))?;
    for entry in
        std::fs::read_dir(src).with_context(|| format!("读取目录失败: {}", src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("读取文件信息失败: {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("拒绝拷贝符号链接: {}", path.display());
        }
        if metadata.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)
                .with_context(|| format!("拷贝文件失败: {}", path.display()))?;
        }
    }
    Ok(())
}

fn make_staging_dir() -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "omnihub-rdp-install-{}-{}-{}",
        std::process::id(),
        unix_nanos(),
        STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建临时目录失败: {}", dir.display()))?;
    Ok(dir)
}

fn make_backup_dir(providers_root: &Path, provider_id: &str) -> PathBuf {
    providers_root.join(format!(
        ".{provider_id}.install-backup-{}-{}",
        unix_nanos(),
        STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn install_provider_from_archive_installs_and_replaces() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = build_provider_archive(temp.path(), "vnc", Some(provider_json()));
        let providers_root = temp.path().join("providers");

        let target = install_provider_from_archive(&archive, &providers_root).unwrap();

        assert!(target.join(PROVIDER_MANIFEST_FILE).exists());
        let loaded = remote_desktop::RemoteDesktopProviderRegistry::load_provider_from_dir(&target)
            .unwrap()
            .expect("插件应能加载");
        assert_eq!("vnc", loaded.id);
        assert_eq!("1.2.3", loaded.version);

        // 重复安装走替换路径，也应成功
        let target = install_provider_from_archive(&archive, &providers_root).unwrap();
        assert!(target.join(PROVIDER_MANIFEST_FILE).exists());
        let loaded =
            remote_desktop::RemoteDesktopProviderRegistry::load_provider_from_dir(&target).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn install_provider_from_archive_rejects_package_without_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = build_provider_archive(temp.path(), "vnc", None);
        let providers_root = temp.path().join("providers");

        assert!(install_provider_from_archive(&archive, &providers_root).is_err());
    }

    fn provider_json() -> &'static str {
        r#"{
            "id": "vnc",
            "name": "VNC",
            "description": "VNC provider",
            "version": "1.2.3",
            "protocol": "vnc",
            "entry": { "command": "./omnihub-vnc-helper" },
            "capabilities": { "resize": "remote_resize", "clipboard_text": true, "cursor_shape": true, "audio": false, "file_transfer": false }
        }"#
    }

    fn build_provider_archive(dir: &Path, id: &str, manifest: Option<&str>) -> PathBuf {
        let archive_path = dir.join(format!("{id}-provider.tar.gz"));
        let file = std::fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        if let Some(manifest) = manifest {
            append_file(
                &mut builder,
                &format!("{id}/{PROVIDER_MANIFEST_FILE}"),
                manifest.as_bytes(),
            );
        }
        append_file(
            &mut builder,
            &format!("{id}/omnihub-{id}-helper"),
            b"helper",
        );
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        archive_path
    }

    fn append_file<W: Write>(builder: &mut tar::Builder<W>, name: &str, bytes: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        builder.append(&header, bytes).unwrap();
    }
}
