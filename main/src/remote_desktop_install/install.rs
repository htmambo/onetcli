use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, anyhow};
use rust_i18n::t;

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
    std::fs::create_dir_all(providers_root).with_context(|| {
        t!(
            "RemoteDesktopInstall.create_provider_dir_failed",
            path = providers_root.display().to_string()
        )
        .to_string()
    })?;

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
            Err(anyhow!(
                t!(
                    "RemoteDesktopInstall.provider_manifest_missing",
                    path = target.display().to_string()
                )
                .to_string()
            ))
        }
        Err(error) => {
            restore_failed_install(&target, backup.as_deref())?;
            Err(error)
        }
    }
}

fn extract_archive(archive: &Path, staging: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive).with_context(|| {
        t!(
            "RemoteDesktopInstall.open_provider_archive_failed",
            path = archive.display().to_string()
        )
        .to_string()
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar
        .entries()
        .context(t!("RemoteDesktopInstall.read_tar_entries_failed").to_string())?
    {
        let mut entry =
            entry.context(t!("RemoteDesktopInstall.read_tar_entries_failed").to_string())?;
        validate_tar_entry(&entry)?;
        let unpacked = entry.unpack_in(staging).with_context(|| {
            t!(
                "RemoteDesktopInstall.unpack_provider_archive_failed",
                path = staging.display().to_string()
            )
            .to_string()
        })?;
        if !unpacked {
            anyhow::bail!(t!("RemoteDesktopInstall.tar_entry_out_of_bounds").to_string());
        }
    }
    Ok(())
}

fn validate_tar_entry<R: std::io::Read>(entry: &tar::Entry<'_, R>) -> anyhow::Result<()> {
    let path = entry
        .path()
        .context(t!("RemoteDesktopInstall.read_tar_entry_path_failed").to_string())?;
    if path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        anyhow::bail!(
            t!(
                "RemoteDesktopInstall.tar_entry_path_out_of_bounds",
                path = path.display().to_string()
            )
            .to_string()
        );
    }
    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        anyhow::bail!(
            t!(
                "RemoteDesktopInstall.tar_entry_symlink_forbidden",
                path = path.display().to_string()
            )
            .to_string()
        );
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
        [] => Err(anyhow!(
            t!(
                "RemoteDesktopInstall.package_missing_manifest",
                file = PROVIDER_MANIFEST_FILE
            )
            .to_string()
        )),
        _ => Err(anyhow!(
            t!("RemoteDesktopInstall.package_multiple_provider_dirs").to_string()
        )),
    }
}

fn is_ignored_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name == ".DS_Store" || name == "__MACOSX" || name.starts_with("._")
}

fn read_provider_id(package_root: &Path) -> anyhow::Result<String> {
    let manifest_path = package_root.join(PROVIDER_MANIFEST_FILE);
    let content = std::fs::read_to_string(&manifest_path).with_context(|| {
        t!(
            "RemoteDesktopInstall.read_provider_manifest_failed",
            path = manifest_path.display().to_string()
        )
        .to_string()
    })?;
    let manifest: serde_json::Value = serde_json::from_str(&content).with_context(|| {
        t!(
            "RemoteDesktopInstall.parse_provider_manifest_failed",
            path = manifest_path.display().to_string()
        )
        .to_string()
    })?;
    let id = manifest
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(t!("RemoteDesktopInstall.provider_manifest_missing_id").to_string())
        })?;
    if id == "." || id == ".." || id.contains('/') || id.contains('\\') {
        anyhow::bail!(
            t!(
                "RemoteDesktopInstall.provider_id_invalid",
                id = id.to_string()
            )
            .to_string()
        );
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
    std::fs::rename(target, &backup).with_context(|| {
        t!(
            "RemoteDesktopInstall.backup_existing_provider_failed",
            path = target.display().to_string()
        )
        .to_string()
    })?;
    Ok(Some(backup))
}

fn restore_failed_install(target: &Path, backup: Option<&Path>) -> anyhow::Result<()> {
    let _ = std::fs::remove_dir_all(target);
    if let Some(backup) = backup {
        std::fs::rename(backup, target).with_context(|| {
            t!(
                "RemoteDesktopInstall.restore_old_provider_failed",
                path = target.display().to_string()
            )
            .to_string()
        })?;
    }
    Ok(())
}

fn remove_install_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        if let Err(_error) = std::fs::remove_dir_all(backup) {
            tracing::warn!(
                "{}",
                t!(
                    "RemoteDesktopInstall.remove_install_backup_failed",
                    path = backup.display().to_string()
                )
            );
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst).with_context(|| {
        t!(
            "RemoteDesktopInstall.create_directory_failed",
            path = dst.display().to_string()
        )
        .to_string()
    })?;
    for entry in std::fs::read_dir(src).with_context(|| {
        t!(
            "RemoteDesktopInstall.read_directory_failed",
            path = src.display().to_string()
        )
        .to_string()
    })? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&path).with_context(|| {
            t!(
                "RemoteDesktopInstall.read_file_metadata_failed",
                path = path.display().to_string()
            )
            .to_string()
        })?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                t!(
                    "RemoteDesktopInstall.symlink_copy_rejected",
                    path = path.display().to_string()
                )
                .to_string()
            );
        }
        if metadata.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target).with_context(|| {
                t!(
                    "RemoteDesktopInstall.copy_file_failed",
                    path = path.display().to_string()
                )
                .to_string()
            })?;
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
    std::fs::create_dir_all(&dir).with_context(|| {
        t!(
            "RemoteDesktopInstall.create_staging_dir_failed",
            path = dir.display().to_string()
        )
        .to_string()
    })?;
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
