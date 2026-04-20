use super::{find_workspace_themes_dir_for_exe, is_theme_file};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间不应早于 UNIX 纪元")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}"))
}

#[test]
fn cargo_target下的可执行文件解析到工作区themes目录() {
    let workspace_dir = unique_temp_dir("one-core-theme-dev");
    let themes_dir = workspace_dir.join("themes");
    std::fs::create_dir_all(&themes_dir).expect("创建 themes 目录失败");
    std::fs::write(workspace_dir.join("Cargo.toml"), "[workspace]\n")
        .expect("写入 Cargo.toml 失败");
    std::fs::write(themes_dir.join("sample.json"), "{}").expect("写入主题文件失败");

    let exe_path = workspace_dir.join("target/debug/onetcli");
    let resolved = find_workspace_themes_dir_for_exe(&exe_path);

    assert_eq!(Some(themes_dir.clone()), resolved);

    std::fs::remove_dir_all(&workspace_dir).expect("清理临时目录失败");
}

#[test]
fn 非工作区target路径不会误判为开发态themes目录() {
    let workspace_dir = unique_temp_dir("one-core-theme-release");
    std::fs::create_dir_all(&workspace_dir).expect("创建临时目录失败");

    let exe_path = workspace_dir.join("bin/onetcli");
    let resolved = find_workspace_themes_dir_for_exe(&exe_path);

    assert_eq!(None, resolved);

    std::fs::remove_dir_all(&workspace_dir).expect("清理临时目录失败");
}

#[test]
fn 缺少主题文件时不返回开发态themes目录() {
    let workspace_dir = unique_temp_dir("one-core-theme-empty");
    let themes_dir = workspace_dir.join("themes");
    std::fs::create_dir_all(&themes_dir).expect("创建 themes 目录失败");
    std::fs::write(workspace_dir.join("Cargo.toml"), "[workspace]\n")
        .expect("写入 Cargo.toml 失败");

    let exe_path = workspace_dir.join("target/debug/onetcli");
    let resolved = find_workspace_themes_dir_for_exe(&exe_path);

    assert_eq!(None, resolved);

    std::fs::remove_dir_all(&workspace_dir).expect("清理临时目录失败");
}

#[test]
fn cargo_target下的可执行文件解析到工作区jsonc主题目录() {
    let workspace_dir = unique_temp_dir("one-core-theme-dev-jsonc");
    let themes_dir = workspace_dir.join("themes");
    std::fs::create_dir_all(&themes_dir).expect("创建 themes 目录失败");
    std::fs::write(workspace_dir.join("Cargo.toml"), "[workspace]\n")
        .expect("写入 Cargo.toml 失败");
    std::fs::write(themes_dir.join("sample.jsonc"), "{ // comment\n }\n")
        .expect("写入主题文件失败");

    let exe_path = workspace_dir.join("target/debug/onetcli");
    let resolved = find_workspace_themes_dir_for_exe(&exe_path);

    assert_eq!(Some(themes_dir.clone()), resolved);

    std::fs::remove_dir_all(&workspace_dir).expect("清理临时目录失败");
}

#[test]
fn 主题文件扩展名支持json和jsonc() {
    assert!(is_theme_file(PathBuf::from("demo.json").as_path()));
    assert!(is_theme_file(PathBuf::from("demo.jsonc").as_path()));
    assert!(!is_theme_file(PathBuf::from("demo.yaml").as_path()));
}
