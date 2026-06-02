use std::time::Duration;

use gpui::App;
use smol::Timer;
use sysinfo::{Pid, System};

/// 系统监控全局状态。
#[derive(Clone)]
pub struct GlobalSystemMonitor {
    /// 系统总内存 (字节)
    pub total_memory: u64,
    /// 系统已用内存 (字节)
    pub used_memory: u64,
    /// 当前进程内存 (字节)
    pub app_memory: u64,
    /// 全局 CPU 使用率 (0-100)
    pub cpu_usage: f32,
}

impl gpui::Global for GlobalSystemMonitor {}

impl Default for GlobalSystemMonitor {
    fn default() -> Self {
        Self {
            total_memory: 0,
            used_memory: 0,
            app_memory: 0,
            cpu_usage: 0.0,
        }
    }
}

/// 初始化系统监控并启动后台刷新任务。
pub fn init_system_monitor(cx: &mut App) {
    cx.set_global(GlobalSystemMonitor::default());

    cx.spawn(async move |cx| {
        loop {
            Timer::after(Duration::from_secs(2)).await;

            let mut sys = System::new_all();
            sys.refresh_all();

            let pid = Pid::from_u32(std::process::id());
            let app_mem = sys.process(pid).map(|p| p.memory()).unwrap_or(0);

            let next = GlobalSystemMonitor {
                total_memory: sys.total_memory(),
                used_memory: sys.used_memory(),
                app_memory: app_mem,
                cpu_usage: sys.global_cpu_usage(),
            };

            cx.update_global::<GlobalSystemMonitor, _>(|monitor, _| {
                *monitor = next.clone();
            });
        }
    })
    .detach();
}

/// 将字节数格式化为易读文本。
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}
