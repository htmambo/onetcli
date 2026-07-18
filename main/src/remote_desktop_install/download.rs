use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::anyhow;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, http};
use tokio::fs;
use tokio::io::AsyncWriteExt;

const DOWNLOAD_DIR_NAME: &str = "omnihub-rdp-provider";
const BUFFER_SIZE: usize = 8192;

/// 下载远程桌面插件包到临时目录并校验 SHA256，返回本地文件路径。
pub(crate) async fn download_package(
    http_client: Arc<dyn HttpClient>,
    url: &str,
    expected_sha256: &str,
    mut on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> anyhow::Result<PathBuf> {
    let download_dir = std::env::temp_dir().join(DOWNLOAD_DIR_NAME);
    fs::create_dir_all(&download_dir)
        .await
        .map_err(|error| anyhow!("创建下载目录失败: {error}"))?;
    let download_path = download_dir.join(package_file_name(url)?);

    if let Err(error) = download_to_file(http_client, url, &download_path, &mut on_progress).await {
        let _ = fs::remove_file(&download_path).await;
        return Err(error);
    }
    if let Err(error) = crate::update::download::verify_sha256(&download_path, expected_sha256) {
        let _ = fs::remove_file(&download_path).await;
        return Err(anyhow!(error));
    }
    Ok(download_path)
}

async fn download_to_file(
    http_client: Arc<dyn HttpClient>,
    url: &str,
    download_path: &Path,
    on_progress: &mut impl FnMut(u64, Option<u64>),
) -> anyhow::Result<()> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(url)
        .header("Accept", "application/octet-stream")
        .body(AsyncBody::empty())
        .map_err(|error| anyhow!("构建下载请求失败: {error}"))?;
    let response = http_client
        .send(request)
        .await
        .map_err(|error| anyhow!("发送下载请求失败: {error}"))?;
    if !response.status().is_success() {
        anyhow::bail!("远程桌面插件包下载失败: {}", response.status());
    }

    let total_bytes = response
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let mut body = response.into_body();
    let mut file = fs::File::create(download_path)
        .await
        .map_err(|error| anyhow!("创建插件包文件失败: {error}"))?;
    let mut downloaded = 0;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|error| anyhow!("读取插件包数据失败: {error}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .await
            .map_err(|error| anyhow!("写入插件包文件失败: {error}"))?;
        downloaded += read as u64;
        on_progress(downloaded, total_bytes);
    }
    file.flush()
        .await
        .map_err(|error| anyhow!("刷新插件包文件失败: {error}"))?;
    file.sync_all()
        .await
        .map_err(|error| anyhow!("同步插件包文件失败: {error}"))?;
    Ok(())
}

fn package_file_name(url: &str) -> anyhow::Result<String> {
    let uri = http::Uri::try_from(url).map_err(|error| anyhow!("插件包地址非法: {error}"))?;
    uri.path()
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .ok_or_else(|| anyhow!("插件包地址缺少文件名: {url}"))
}
