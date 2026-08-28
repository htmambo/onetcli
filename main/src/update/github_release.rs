use std::sync::Arc;

use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request};
use serde::Deserialize;

use super::UpdateDialogInfo;

const GITHUB_OWNER: &str = "htmambo";
const GITHUB_REPO: &str = "onetcli";
const GITHUB_API_URL: &str = "https://api.github.com/repos/htmambo/onetcli/releases/latest";
pub const GITHUB_LATEST_RELEASE_URL: &str = "https://github.com/htmambo/onetcli/releases/latest";
const GITHUB_USER_AGENT: &str = "omnihub-updater";

const EXPECTED_ARCHIVE_NAME: &str =
    expected_archive_name_for(std::env::consts::OS, std::env::consts::ARCH);

pub(crate) const fn expected_archive_name_for(os: &str, arch: &str) -> &'static str {
    match (os.as_bytes(), arch.as_bytes()) {
        (b"macos", b"aarch64") => "omnihub-aarch64-apple-darwin.tar.gz",
        (b"macos", b"x86_64") => "omnihub-x86_64-apple-darwin.tar.gz",
        (b"linux", b"x86_64") => "omnihub-x86_64-unknown-linux-gnu.tar.gz",
        (b"linux", b"aarch64") => "omnihub-aarch64-unknown-linux-gnu.tar.gz",
        (b"windows", b"x86_64") => "omnihub-x86_64-pc-windows-msvc.zip",
        _ => "",
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubReleaseAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubRelease {
    pub(crate) tag_name: String,
    pub(crate) assets: Vec<GithubReleaseAsset>,
}

pub(crate) async fn fetch_github_release(
    http_client: Arc<dyn HttpClient>,
) -> Result<GithubRelease, String> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(GITHUB_API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", GITHUB_USER_AGENT)
        .body(AsyncBody::empty())
        .map_err(|err| format!("构建 GitHub Release 请求失败: {}", err))?;

    let response = http_client
        .send(request)
        .await
        .map_err(|err| format!("发送 GitHub Release 请求失败: {}", err))?;

    let status = response.status();
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .map_err(|err| format!("读取 GitHub Release 响应失败: {}", err))?;

    if !status.is_success() {
        return Err(format!(
            "GitHub Release 接口返回异常状态码: {} ({}/{})",
            status, GITHUB_OWNER, GITHUB_REPO
        ));
    }

    serde_json::from_slice::<GithubRelease>(&bytes)
        .map_err(|err| format!("解析 GitHub Release 响应失败: {}", err))
}

pub(crate) fn select_github_asset(release: &GithubRelease) -> Option<&GithubReleaseAsset> {
    if EXPECTED_ARCHIVE_NAME.is_empty() {
        return None;
    }

    release
        .assets
        .iter()
        .find(|asset| asset.name == EXPECTED_ARCHIVE_NAME)
}

pub(crate) fn select_github_sha256_asset(release: &GithubRelease) -> Option<&GithubReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == "sha256sums.txt")
}

/// 解析 sha256sums.txt 内容，返回 file_name 对应的 64 位十六进制哈希。
/// 兼容 `hash  file` 与 `hash *file`（sha256sum 二进制模式标记）两种格式。
fn parse_sha256_for(sha256sums: &str, file_name: &str) -> Option<String> {
    sha256sums.lines().find_map(|line| {
        let mut parts = line.trim().splitn(2, char::is_whitespace);
        let hash = parts.next().unwrap_or("");
        let name = parts.next()?.trim_start_matches('*').trim();
        if name == file_name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(hash.to_ascii_lowercase())
        } else {
            None
        }
    })
}

/// 拉取并解析 Release 的 sha256sums.txt，取当前平台安装包的哈希。
/// 任一环节失败仅记 warn 并返回 None（降级为不校验，不阻断更新流程）。
pub(crate) async fn fetch_github_sha256(
    http_client: Arc<dyn HttpClient>,
    release: &GithubRelease,
) -> Option<String> {
    let asset = select_github_sha256_asset(release)?;
    let request = Request::builder()
        .method(Method::GET)
        .uri(asset.browser_download_url.as_str())
        .header("User-Agent", GITHUB_USER_AGENT)
        .body(AsyncBody::empty())
        .ok()?;
    let mut response = http_client.send(request).await.ok()?;
    if !response.status().is_success() {
        tracing::warn!("拉取 sha256sums.txt 返回异常状态码: {}", response.status());
        return None;
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes).await.ok()?;
    let text = String::from_utf8(bytes).ok()?;
    match parse_sha256_for(&text, EXPECTED_ARCHIVE_NAME) {
        Some(hash) => Some(hash),
        None => {
            tracing::warn!("sha256sums.txt 中未找到 {} 的校验值", EXPECTED_ARCHIVE_NAME);
            None
        }
    }
}

pub(crate) fn github_release_to_dialog_info(
    release: &GithubRelease,
    current_version: &str,
) -> Result<UpdateDialogInfo, String> {
    let asset = select_github_asset(release)
        .ok_or_else(|| format!("未找到当前平台的发布资产: {}", EXPECTED_ARCHIVE_NAME))?;

    let release_page_url = GITHUB_LATEST_RELEASE_URL.to_string();

    Ok(UpdateDialogInfo {
        current_version: current_version.to_string(),
        latest_version: release.tag_name.clone(),
        download_url: Some(asset.browser_download_url.clone()),
        fallback_download_url: None,
        expected_sha256: None,
        release_page_url: Some(release_page_url),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::anyhow;
    use gpui::http_client::{HttpClient, http};

    use super::*;
    use crate::update::test_support::FakeHttpClient;

    #[tokio::test]
    async fn fetch_github_release_sends_expected_request() {
        let client = Arc::new(FakeHttpClient::new(vec![FakeHttpClient::response(
            200,
            r#"{
                "tag_name":"v1.2.3",
                "body":"release notes",
                "assets":[]
            }"#,
        )]));
        let http_client: Arc<dyn HttpClient> = client.clone();

        let release = fetch_github_release(http_client)
            .await
            .expect("GitHub Release 请求应成功");

        assert_eq!(release.tag_name, "v1.2.3");

        let requests = client.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, http::Method::GET);
        assert_eq!(requests[0].uri, GITHUB_API_URL);
        assert_eq!(requests[0].user_agent.as_deref(), Some(GITHUB_USER_AGENT));
    }

    #[test]
    fn github_release_to_dialog_info_uses_matching_asset() {
        let release = GithubRelease {
            tag_name: "v1.2.3".to_string(),
            assets: vec![
                GithubReleaseAsset {
                    name: "sha256sums.txt".to_string(),
                    browser_download_url: "https://example.com/sha256".to_string(),
                },
                GithubReleaseAsset {
                    name: EXPECTED_ARCHIVE_NAME.to_string(),
                    browser_download_url: "https://example.com/update".to_string(),
                },
            ],
        };

        let info = github_release_to_dialog_info(&release, "0.1.0").expect("应选择当前平台资产");

        assert_eq!(info.latest_version, "v1.2.3");
        assert_eq!(info.current_version, "0.1.0");
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://example.com/update")
        );
    }

    #[test]
    fn expected_archive_name_includes_linux_arm64() {
        assert_eq!(
            "omnihub-aarch64-unknown-linux-gnu.tar.gz",
            expected_archive_name_for("linux", "aarch64")
        );
    }

    #[tokio::test]
    async fn fetch_github_release_returns_error_on_transport_failure() {
        let client = Arc::new(FakeHttpClient::new(vec![Err(anyhow!("network down"))]));
        let http_client: Arc<dyn HttpClient> = client;

        let err = fetch_github_release(http_client)
            .await
            .expect_err("传输失败应返回错误");

        assert!(err.contains("发送 GitHub Release 请求失败"));
    }

    #[test]
    fn parse_sha256_for_handles_standard_binary_and_garbage_lines() {
        let archive = EXPECTED_ARCHIVE_NAME;
        let hash = "a".repeat(64);

        // 标准两空格格式
        let text = format!("{hash}  {archive}\nother  file.zip\n");
        assert_eq!(
            parse_sha256_for(&text, archive).as_deref(),
            Some(hash.as_str())
        );

        // 二进制模式 `*file` + 制表符分隔 + 大写哈希归一化
        let text = format!("{}\t*{archive}\r\n", "A".repeat(64));
        assert_eq!(
            parse_sha256_for(&text, archive).as_deref(),
            Some(hash.as_str())
        );

        // 哈希长度不足 / 文件名不匹配 / 空内容
        assert_eq!(parse_sha256_for("short  file", archive), None);
        assert_eq!(parse_sha256_for(&text, "nope.bin"), None);
        assert_eq!(parse_sha256_for("", archive), None);
    }

    #[tokio::test]
    async fn fetch_github_sha256_reads_matching_hash_from_sums_asset() {
        let archive = EXPECTED_ARCHIVE_NAME;
        let hash = "b".repeat(64);
        let release_json = format!(
            r#"{{"tag_name":"v1.2.3","assets":[{{"name":"sha256sums.txt","browser_download_url":"https://example.com/sha256"}},{{"name":"{archive}","browser_download_url":"https://example.com/update"}}]}}"#
        );
        let client = Arc::new(FakeHttpClient::new(vec![
            FakeHttpClient::response(200, release_json.as_str()),
            FakeHttpClient::response(200, &format!("{hash}  {archive}\n")),
        ]));
        let http_client: Arc<dyn HttpClient> = client.clone();

        let release = fetch_github_release(http_client.clone())
            .await
            .expect("release 请求应成功");
        let parsed = fetch_github_sha256(http_client, &release)
            .await
            .expect("应解析出当前平台哈希");
        assert_eq!(parsed, hash);

        let requests = client.take_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].uri, "https://example.com/sha256");
    }

    #[tokio::test]
    async fn fetch_github_sha256_returns_none_without_sums_asset() {
        let client = Arc::new(FakeHttpClient::new(vec![FakeHttpClient::response(
            200,
            r#"{"tag_name":"v1.2.3","assets":[]}"#,
        )]));
        let http_client: Arc<dyn HttpClient> = client;

        let release = fetch_github_release(http_client)
            .await
            .expect("release 请求应成功");
        assert_eq!(
            fetch_github_sha256(Arc::new(FakeHttpClient::new(vec![])), &release).await,
            None
        );
    }
}
