//! 把远程文件时间戳恢复到本地副本。
//!
//! SFTP v3 只提供秒级 `atime` / `mtime`；创建时间不在协议内，无法恢复。

use crate::FileEntry;
use filetime::FileTime;
use rust_i18n::t;
use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 远程文件时间戳。字段为 `None` 表示服务端未提供该时间。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RemoteTimes {
    pub(crate) atime: Option<SystemTime>,
    pub(crate) mtime: Option<SystemTime>,
}

impl RemoteTimes {
    /// 由 SFTP 属性构造。`None` 与 0（部分服务端用 0 表示未知）都视为未提供。
    pub(crate) fn from_sftp_attrs(atime: Option<u32>, mtime: Option<u32>) -> Self {
        Self {
            atime: atime
                .filter(|secs| *secs != 0)
                .map(unix_secs_to_system_time),
            mtime: mtime
                .filter(|secs| *secs != 0)
                .map(unix_secs_to_system_time),
        }
    }

    /// 目录下载场景复用 `FileEntry`。
    ///
    /// `FileEntry::modified` 在服务端未提供 mtime（或返回 0 哨兵）时已回退为
    /// 列表时刻，因此这里恒为 `Some`；对本地副本的效果等同于"不修改"。
    pub(crate) fn from_file_entry(entry: &FileEntry) -> Self {
        Self {
            atime: entry.atime,
            mtime: Some(entry.modified),
        }
    }

    /// 把远程时间应用到已写完并关闭句柄的本地路径。
    ///
    /// mtime 缺失时完全不修改（也不单独设置 atime）；失败只记录 warn，
    /// 不影响传输结果。
    pub(crate) fn apply_to_path(&self, path: &Path) {
        let Some(mtime) = self.mtime else {
            return;
        };

        let result = match self.atime {
            Some(atime) => filetime::set_file_times(path, to_file_time(atime), to_file_time(mtime)),
            None => filetime::set_file_mtime(path, to_file_time(mtime)),
        };

        if let Err(error) = result {
            tracing::warn!(
                "{}",
                t!(
                    "Sftp.apply_times_failed",
                    path = path.display().to_string(),
                    error = error.to_string()
                )
            );
        }
    }
}

/// 在全部子项写完后恢复目录时间：按路径深度从深到浅，父目录最后落定。
///
/// 前置条件：目录内所有子项已创建/写入完成，否则后续写入会再次刷新目录 mtime。
pub(crate) fn apply_dir_times(mut dirs: Vec<(PathBuf, RemoteTimes)>) {
    dirs.sort_by_key(|(path, _)| Reverse(path.components().count()));
    for (path, times) in dirs {
        times.apply_to_path(&path);
    }
}

/// SFTP v3 的秒级时间戳转 `SystemTime`。
pub(crate) fn unix_secs_to_system_time(secs: u32) -> SystemTime {
    // u32 上限约 2106 年，主流平台的 SystemTime 均可表示；回退到 now 仅为
    // 避免把不可达的溢出写成 1970（1970 在本模块代表"未知时间"）。
    UNIX_EPOCH
        .checked_add(Duration::from_secs(u64::from(secs)))
        .unwrap_or_else(SystemTime::now)
}

fn to_file_time(time: SystemTime) -> FileTime {
    FileTime::from_system_time(time)
}

#[cfg(test)]
mod tests {
    use super::{RemoteTimes, apply_dir_times, unix_secs_to_system_time};
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::TempDir;

    const SAMPLE_SECS: u32 = 1_700_000_000;
    /// FAT/exFAT 等文件系统按 2 秒粒度截断时间。
    const FS_TOLERANCE: Duration = Duration::from_secs(2);

    fn sample_time() -> std::time::SystemTime {
        UNIX_EPOCH + Duration::from_secs(u64::from(SAMPLE_SECS))
    }

    fn assert_time_close(actual: std::time::SystemTime, expected: std::time::SystemTime) {
        let diff = match actual.duration_since(expected) {
            Ok(diff) => diff,
            Err(error) => error.duration(),
        };
        assert!(
            diff <= FS_TOLERANCE,
            "mtime 偏差过大: {actual:?} vs {expected:?}"
        );
    }

    fn mtime_of(path: &Path) -> std::time::SystemTime {
        fs::metadata(path)
            .expect("应能读取 metadata")
            .modified()
            .expect("应有 mtime")
    }

    #[test]
    fn unix_secs_maps_to_system_time() {
        assert_eq!(unix_secs_to_system_time(0), UNIX_EPOCH);
        assert_eq!(unix_secs_to_system_time(SAMPLE_SECS), sample_time());
    }

    #[test]
    fn from_sftp_attrs_keeps_missing_fields_none() {
        let times = RemoteTimes::from_sftp_attrs(None, None);
        assert_eq!(times, RemoteTimes::default());
        assert_eq!(times.atime, None);
        assert_eq!(times.mtime, None);
    }

    #[test]
    fn from_sftp_attrs_treats_zero_as_unknown() {
        let times = RemoteTimes::from_sftp_attrs(Some(0), Some(0));
        assert_eq!(times, RemoteTimes::default());
    }

    #[test]
    fn from_sftp_attrs_converts_provided_seconds() {
        let times = RemoteTimes::from_sftp_attrs(Some(SAMPLE_SECS), Some(SAMPLE_SECS - 10));
        assert_eq!(times.atime, Some(sample_time()));
        assert_eq!(
            times.mtime,
            Some(UNIX_EPOCH + Duration::from_secs(u64::from(SAMPLE_SECS - 10)))
        );
    }

    #[test]
    fn apply_to_path_sets_mtime() {
        let dir = TempDir::new().expect("应能创建临时目录");
        let file = dir.path().join("download.bin");
        fs::write(&file, b"content").expect("应能写入临时文件");

        RemoteTimes::from_sftp_attrs(None, Some(SAMPLE_SECS)).apply_to_path(&file);

        assert_time_close(mtime_of(&file), sample_time());
        assert_eq!(fs::read(&file).expect("应能读取"), b"content");
    }

    #[test]
    fn from_file_entry_takes_entry_times() {
        let entry = super::FileEntry {
            name: "a.txt".to_string(),
            path: "/remote/a.txt".to_string(),
            size: 1,
            modified: sample_time(),
            atime: Some(sample_time()),
            is_dir: false,
            permissions: 0o644,
        };

        let times = RemoteTimes::from_file_entry(&entry);

        assert_eq!(times.mtime, Some(sample_time()));
        assert_eq!(times.atime, Some(sample_time()));
    }

    #[test]
    fn apply_with_both_times_sets_mtime() {
        let dir = TempDir::new().expect("应能创建临时目录");
        let file = dir.path().join("both-times.bin");
        fs::write(&file, b"content").expect("应能写入临时文件");

        RemoteTimes::from_sftp_attrs(Some(SAMPLE_SECS), Some(SAMPLE_SECS)).apply_to_path(&file);

        // atime 在 relatime 挂载下不可靠，只断言 mtime 分支生效。
        assert_time_close(mtime_of(&file), sample_time());
    }

    #[test]
    fn apply_with_only_atime_is_noop() {
        let dir = TempDir::new().expect("应能创建临时目录");
        let file = dir.path().join("only-atime.bin");
        fs::write(&file, b"content").expect("应能写入临时文件");
        let before = mtime_of(&file);

        RemoteTimes {
            atime: Some(sample_time()),
            mtime: None,
        }
        .apply_to_path(&file);

        assert_eq!(mtime_of(&file), before);
    }

    #[test]
    fn apply_without_mtime_leaves_file_untouched() {
        let dir = TempDir::new().expect("应能创建临时目录");
        let file = dir.path().join("untouched.bin");
        fs::write(&file, b"content").expect("应能写入临时文件");
        let before = mtime_of(&file);

        RemoteTimes::default().apply_to_path(&file);

        assert_eq!(mtime_of(&file), before);
    }

    #[test]
    fn apply_to_missing_path_does_not_panic() {
        let dir = TempDir::new().expect("应能创建临时目录");
        RemoteTimes::from_sftp_attrs(None, Some(SAMPLE_SECS))
            .apply_to_path(&dir.path().join("missing.bin"));
    }

    #[test]
    fn apply_dir_times_sets_nested_and_base() {
        let dir = TempDir::new().expect("应能创建临时目录");
        let base = dir.path().join("root");
        let nested = base.join("a/b");
        let empty = base.join("empty");
        fs::create_dir_all(&nested).expect("应能创建嵌套目录");
        fs::create_dir_all(&empty).expect("应能创建空目录");
        fs::write(nested.join("file.txt"), b"x").expect("应能写入文件");

        let times = RemoteTimes::from_sftp_attrs(None, Some(SAMPLE_SECS));
        apply_dir_times(vec![
            (base.clone(), times),
            (nested.clone(), times),
            (empty.clone(), times),
            (base.join("a"), times),
        ]);

        for path in [&base, &nested, &empty, &base.join("a")] {
            assert_time_close(mtime_of(path), sample_time());
        }
    }
}
