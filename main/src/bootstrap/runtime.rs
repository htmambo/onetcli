use rust_i18n::t;
use tokio::runtime::Runtime;

/// 处理启动前的命令行分流。
pub fn handle_startup_command() -> bool {
    #[cfg(unix)]
    {
        if std::env::args().any(|arg| arg == "--local-pty-host") {
            let rt = Runtime::new()
                .unwrap_or_else(|_| panic!("{}", t!("Bootstrap.tokio_runtime_create_failed")));
            if let Err(err) = rt.block_on(terminal::run_local_pty_host()) {
                eprintln!(
                    "{}",
                    t!(
                        "Bootstrap.local_pty_host_start_failed",
                        err = format!("{err}")
                    )
                );
                std::process::exit(1);
            }
            return true;
        }
    }

    false
}
