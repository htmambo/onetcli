use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request};
use serde::Deserialize;

const PROVIDER_KIND: &str = "remote_desktop_provider";
const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/feigeCode/onetcli-extensions/main/manifest.json";
const MANIFEST_URL_ENV: &str = "ONETCLI_EXTENSION_MANIFEST_URL";
const RELEASE_DOWNLOAD_BASE: &str =
    "https://github.com/feigeCode/onetcli-extensions/releases/download";

#[derive(Debug, Deserialize)]
pub(crate) struct MarketplaceManifest {
    #[serde(default)]
    extensions: Vec<MarketplaceEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MarketplaceEntry {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) release_tag: String,
    #[serde(default)]
    pub(crate) artifacts: HashMap<String, MarketplaceArtifact>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MarketplaceArtifact {
    pub(crate) file: String,
    #[serde(default)]
    pub(crate) sha256: String,
}

#[derive(Debug)]
pub(crate) struct ProviderPackage {
    pub(crate) version: String,
    pub(crate) download_url: String,
    pub(crate) sha256: String,
}

/// 从扩展市场解析指定远程桌面插件的下载信息。
/// 先取顶层 manifest 拿到 release_tag，再取发行清单选择当前平台的构建产物。
pub(crate) async fn fetch_provider_package(
    http_client: Arc<dyn HttpClient>,
    provider_id: &str,
) -> anyhow::Result<ProviderPackage> {
    let manifest = fetch_manifest(http_client.clone(), &manifest_url()).await?;
    let entry = find_provider_entry(&manifest, provider_id)
        .ok_or_else(|| anyhow!("扩展市场未找到远程桌面插件 {provider_id}"))?;
    if entry.release_tag.trim().is_empty() {
        anyhow::bail!("远程桌面插件 {provider_id} 缺少 release_tag");
    }
    let sub_manifest_url = release_url(&entry.release_tag, "extension-manifest.json");
    let sub_manifest = fetch_manifest(http_client, &sub_manifest_url).await?;
    let sub_entry = find_provider_entry(&sub_manifest, provider_id)
        .ok_or_else(|| anyhow!("远程桌面插件 {provider_id} 缺少发行清单"))?;
    tracing::debug!(
        provider = %sub_entry.name,
        version = %sub_entry.version,
        "已解析远程桌面插件发行清单"
    );
    select_provider_package(sub_entry, &entry.release_tag)
}

fn find_provider_entry<'a>(
    manifest: &'a MarketplaceManifest,
    provider_id: &str,
) -> Option<&'a MarketplaceEntry> {
    manifest
        .extensions
        .iter()
        .find(|entry| entry.kind == PROVIDER_KIND && entry.id == provider_id)
}

fn select_provider_package(
    entry: &MarketplaceEntry,
    release_tag: &str,
) -> anyhow::Result<ProviderPackage> {
    let artifact = marketplace_target_keys()
        .iter()
        .find_map(|key| entry.artifacts.get(*key))
        .ok_or_else(|| anyhow!("远程桌面插件 {} 没有适配当前平台的构建产物", entry.id))?;
    if artifact.file.trim().is_empty() {
        anyhow::bail!("远程桌面插件 {} 缺少构建产物文件名", entry.id);
    }
    if artifact.sha256.trim().is_empty() {
        anyhow::bail!("远程桌面插件 {} 缺少 SHA256 校验值", entry.id);
    }
    Ok(ProviderPackage {
        version: entry.version.clone(),
        download_url: release_url(release_tag, &artifact.file),
        sha256: artifact.sha256.clone(),
    })
}

fn manifest_url() -> String {
    std::env::var(MANIFEST_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_string())
}

fn release_url(release_tag: &str, file: &str) -> String {
    format!("{RELEASE_DOWNLOAD_BASE}/{release_tag}/{file}")
}

async fn fetch_manifest(
    http_client: Arc<dyn HttpClient>,
    url: &str,
) -> anyhow::Result<MarketplaceManifest> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(url)
        .body(AsyncBody::empty())
        .map_err(|error| anyhow!("构建清单请求失败: {error}"))?;
    let response = http_client
        .send(request)
        .await
        .map_err(|error| anyhow!("请求扩展市场清单失败: {error}"))?;
    if !response.status().is_success() {
        anyhow::bail!("请求扩展市场清单失败: {}", response.status());
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .map_err(|error| anyhow!("读取扩展市场清单失败: {error}"))?;
    serde_json::from_slice(&bytes).context("解析扩展市场清单失败")
}

fn marketplace_target_keys() -> &'static [&'static str] {
    marketplace_target_keys_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn marketplace_target_keys_for(os: &str, arch: &str) -> &'static [&'static str] {
    match (os, arch) {
        ("macos", "aarch64") => &["aarch64-apple-darwin", "macos", "universal"],
        ("macos", "x86_64") => &["x86_64-apple-darwin", "macos", "universal"],
        ("linux", "x86_64") => &["x86_64-unknown-linux-gnu", "linux", "universal"],
        ("linux", "aarch64") => &["aarch64-unknown-linux-gnu", "linux", "universal"],
        ("windows", "x86_64") => &["x86_64-pc-windows-msvc", "windows", "universal"],
        _ => &["universal"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VNC_SUB_MANIFEST: &str = r#"{"schema_version":2,"release_version":"vnc-v0.1.1","extensions":[{"id":"vnc","kind":"remote_desktop_provider","name":"VNC","version":"0.1.1","release_tag":"vnc-v0.1.1","artifacts":{"aarch64-apple-darwin":{"file":"vnc-remote-desktop-provider-aarch64-apple-darwin.tar.gz","sha256":"bc45"},"x86_64-unknown-linux-gnu":{"file":"vnc-remote-desktop-provider-x86_64-unknown-linux-gnu.tar.gz","sha256":"30f44123ba1e3f1d1a322f69f96577f38b8d4fa9de1be1785b8926054d5489c4"}}}]}"#;

    #[test]
    fn marketplace_target_keys_cover_all_platforms() {
        assert_eq!(
            &["aarch64-apple-darwin", "macos", "universal"],
            marketplace_target_keys_for("macos", "aarch64")
        );
        assert_eq!(
            &["x86_64-apple-darwin", "macos", "universal"],
            marketplace_target_keys_for("macos", "x86_64")
        );
        assert_eq!(
            &["x86_64-unknown-linux-gnu", "linux", "universal"],
            marketplace_target_keys_for("linux", "x86_64")
        );
        assert_eq!(
            &["aarch64-unknown-linux-gnu", "linux", "universal"],
            marketplace_target_keys_for("linux", "aarch64")
        );
        assert_eq!(
            &["x86_64-pc-windows-msvc", "windows", "universal"],
            marketplace_target_keys_for("windows", "x86_64")
        );
        assert_eq!(
            &["universal"],
            marketplace_target_keys_for("linux", "riscv64")
        );
    }

    #[test]
    fn find_provider_entry_matches_kind_and_id() {
        let manifest: MarketplaceManifest = serde_json::from_str(VNC_SUB_MANIFEST).unwrap();

        assert_eq!("vnc", find_provider_entry(&manifest, "vnc").unwrap().id);
        assert!(find_provider_entry(&manifest, "rdp").is_none());
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn select_provider_package_picks_linux_x86_64_artifact() {
        let manifest: MarketplaceManifest = serde_json::from_str(VNC_SUB_MANIFEST).unwrap();
        let entry = find_provider_entry(&manifest, "vnc").unwrap();

        let package = select_provider_package(entry, "vnc-v0.1.1").unwrap();

        assert_eq!("0.1.1", package.version);
        assert_eq!(
            "https://github.com/feigeCode/onetcli-extensions/releases/download/vnc-v0.1.1/vnc-remote-desktop-provider-x86_64-unknown-linux-gnu.tar.gz",
            package.download_url
        );
        assert_eq!(
            "30f44123ba1e3f1d1a322f69f96577f38b8d4fa9de1be1785b8926054d5489c4",
            package.sha256
        );
    }

    #[test]
    fn select_provider_package_requires_sha256() {
        let json = r#"{"extensions":[{"id":"vnc","kind":"remote_desktop_provider","version":"0.1.1","artifacts":{"universal":{"file":"vnc.tar.gz","sha256":""}}}]}"#;
        let manifest: MarketplaceManifest = serde_json::from_str(json).unwrap();
        let entry = find_provider_entry(&manifest, "vnc").unwrap();

        assert!(select_provider_package(entry, "vnc-v0.1.1").is_err());
    }
}
