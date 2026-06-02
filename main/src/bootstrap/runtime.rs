use tokio::runtime::Runtime;

/// 处理启动前的命令行分流。
pub fn handle_startup_command() -> bool {
    #[cfg(unix)]
    {
        if std::env::args().any(|arg| arg == "--local-pty-host") {
            let rt = Runtime::new().expect("创建 Tokio runtime 失败");
            if let Err(err) = rt.block_on(terminal::run_local_pty_host()) {
                eprintln!("local-pty-host 启动失败: {err}");
                std::process::exit(1);
            }
            return true;
        }
    }

    false
}
