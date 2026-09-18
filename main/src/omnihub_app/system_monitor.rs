use std::time::Duration;

use gpui::App;
use smol::Timer;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind,
    System,
};

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
///
/// P1：只刷新 memory + cpu_usage + 单进程 pid，避免 `System::new_all()` +
/// `refresh_all()` 枚举全部进程/磁盘/网络。这两项是 sysinfo 0.32 里耗时
/// 最重的操作，每 2s 调用会把主线程阻塞数十毫秒。
pub fn init_system_monitor(cx: &mut App) {
    cx.set_global(GlobalSystemMonitor::default());

    cx.spawn(async move |cx| {
        // 第一次唤醒预热 System：sysinfo 需要一次完整基线采集才能计算 cpu_usage 差值。
        let mut sys = build_system();
        refresh_system(&mut sys);
        publish_sample(&cx, &sys);

        loop {
            Timer::after(Duration::from_secs(2)).await;
            refresh_system(&mut sys);
            publish_sample(&cx, &sys);
        }
    })
    .detach();
}

/// 构造仅启用 memory/cpu 子集的 `System`（不枚举磁盘、网络、全部进程）。
fn build_system() -> System {
    System::new_with_specifics(
        RefreshKind::default()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::everything()),
    )
}

/// 增量刷新：只更新 memory、cpu_usage、本进程 pid 内存。
fn refresh_system(sys: &mut System) {
    sys.refresh_specifics(
        RefreshKind::default()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::new().with_cpu_usage()),
    );
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        false,
        ProcessRefreshKind::new().with_memory(),
    );
}

/// 采样当前 System 状态并发布到全局。
fn publish_sample(cx: &gpui::AsyncApp, sys: &System) {
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
