use std::path::Path;

use rust_i18n::t;

pub(crate) fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|err| {
        t!(
            "UpdateExtract.create_extract_dir_failed",
            err = err.to_string()
        )
        .to_string()
    })?;

    let file_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            t!(
                "UpdateExtract.archive_filename_unrecognized",
                path = archive.display().to_string()
            )
            .to_string()
        })?;

    if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        let file = std::fs::File::open(archive).map_err(|err| {
            t!(
                "UpdateExtract.open_update_archive_failed",
                path = archive.display().to_string(),
                err = err.to_string()
            )
            .to_string()
        })?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(dest_dir).map_err(|err| {
            t!("UpdateExtract.extract_tar_gz_failed", err = err.to_string()).to_string()
        })?;
        return Ok(());
    }

    if file_name.ends_with(".zip") {
        #[cfg(target_os = "windows")]
        {
            let file = std::fs::File::open(archive).map_err(|err| {
                t!(
                    "UpdateExtract.open_update_archive_failed",
                    path = archive.display().to_string(),
                    err = err.to_string()
                )
                .to_string()
            })?;
            let mut zip = zip::ZipArchive::new(file).map_err(|err| {
                t!(
                    "UpdateExtract.read_zip_update_failed",
                    err = err.to_string()
                )
                .to_string()
            })?;
            zip.extract(dest_dir).map_err(|err| {
                t!(
                    "UpdateExtract.extract_zip_update_failed",
                    err = err.to_string()
                )
                .to_string()
            })?;
            return Ok(());
        }

        #[cfg(not(target_os = "windows"))]
        {
            return Err(t!("UpdateExtract.zip_unsupported_on_platform").to_string());
        }
    }

    Err(t!(
        "UpdateExtract.unsupported_archive_format",
        path = archive.display().to_string()
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::Builder;

    use super::extract_archive;

    #[test]
    fn extract_archive_unpacks_tar_gz_contents() {
        let temp_dir = TestDir::new("extract-archive");
        let archive_path = temp_dir.path.join("update.tar.gz");
        let dest_dir = temp_dir.path.join("dest");

        write_tar_gz(
            &archive_path,
            &[("OmniHub.app/Contents/MacOS/omnihub", b"binary".as_slice())],
        );

        let result = extract_archive(&archive_path, &dest_dir);

        assert!(result.is_ok(), "tar.gz 解压应成功: {result:?}");
        let extracted_path = dest_dir.join("OmniHub.app/Contents/MacOS/omnihub");
        let contents = std::fs::read(&extracted_path).expect("应能读取解压后的文件");
        assert_eq!(contents, b"binary");
    }

    fn write_tar_gz(archive_path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(archive_path).expect("创建 tar.gz 失败");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        for (path, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, &mut std::io::Cursor::new(contents))
                .expect("写入 tar 条目失败");
        }

        let encoder = builder.into_inner().expect("完成 tar 构建失败");
        encoder.finish().expect("完成 gzip 压缩失败");
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("系统时间异常")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), now));
            std::fs::create_dir_all(&path).expect("创建临时目录失败");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
