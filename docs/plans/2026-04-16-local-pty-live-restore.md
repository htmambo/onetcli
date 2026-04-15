# 本地 PTY 活会话恢复 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为本地终端恢复补上 Tabby 风格的 PTY live attach 能力，让应用重启后优先重新附着仍存活的本地 PTY，会话不可附着时再回退到现有“历史回放 + 新 shell”恢复。

**Architecture:** 当前 `LocalPtyBackend` 直接在 UI 进程内创建 PTY，应用退出时 PTY 会跟随进程死亡，因此无法实现真正的跨重启附着。推荐方案是在同一可执行文件中增加 `--local-pty-host` 子进程模式：host 进程独立持有 PTY master、shell 生命周期和 session registry，UI 进程通过 IPC 读写原始字节流并继续使用现有 `Terminal`/`TerminalView` 渲染；恢复快照新增 `pty_session_id`，启动恢复时先尝试 attach，失败则回退到现有 `buffer_content` 恢复。

**Tech Stack:** Rust workspace、GPUI、Tokio、tokio-util、serde、UUID、Unix Domain Socket / Windows Named Pipe IPC、`portable-pty`（推荐用于 host 侧跨平台 PTY 管理）

---

## 前置决策

1. 第一版只覆盖 `LocalTerminal`，SSH / Serial 不进入本计划。
2. 当前“统一弹窗恢复”继续保留，不新增自动恢复分支。
3. 第一版推荐为 detached PTY 增加生存上限，建议 `10 min` TTL；用户在恢复弹窗中明确“跳过”时，直接销毁对应 detached PTY，避免孤儿 shell 长时间滞留。
4. 第一版不追求“应用关闭期间产生的全部输出都完整无损”；依赖 `buffer_content` 作为稳定回退，同时为 host 端保留后续补 broker-side ring buffer 的扩展点。
5. `cargo test -p terminal --lib` 和 `cargo test -p terminal_view --lib` 当前被 `vendor/zed/crates/gpui/src/platform/test/window.rs` 缺 `disable_ime` 阻塞，实施前需要先解锁这条测试链路。

## 非目标

1. 本计划不改恢复弹窗交互模型。
2. 本计划不把 Tabby 的 closed-tab recovery 栈一并搬进来。
3. 本计划不在第一版引入“后台多窗口共享同一 PTY”。

### Task 0: 解锁终端相关单元测试链路

**Files:**
- Modify: `vendor/zed/crates/gpui/src/platform/test/window.rs`
- Test: `cargo test -p terminal --lib`
- Test: `cargo test -p terminal_view --lib`

**Step 1: 为 test window 补齐 `disable_ime` 空实现**

让 `PlatformWindow for TestWindow` 再次和当前 trait 对齐，消除 `E0046` 编译错误。

**Step 2: 先验证测试链路被真正解锁**

Run: `cargo test -p terminal --lib`
Expected: 进入 `terminal` crate 自身测试，而不是卡死在 `gpui` trait 缺项。

**Step 3: 再验证 `terminal_view` 测试也能编译**

Run: `cargo test -p terminal_view --lib`
Expected: 至少进入 `terminal_view` crate 自身测试收集阶段。

**Step 4: 单独提交测试解锁补丁**

Run: `git add vendor/zed/crates/gpui/src/platform/test/window.rs`
Run: `git commit -m "test: unblock terminal crate unit tests"`

### Task 1: 固化 PTY host 协议与关闭语义

**Files:**
- Create: `crates/terminal/src/local_pty_protocol.rs`
- Modify: `crates/terminal/src/types.rs`
- Modify: `crates/terminal/src/lib.rs`
- Test: `crates/terminal/src/local_pty_protocol.rs`

**Step 1: 定义 session 与 IPC 协议模型**

新增明确的协议类型，至少包含：

```rust
pub type LocalPtySessionId = String;

pub enum LocalPtyCloseMode {
    Kill,
    Detach,
}

pub enum LocalPtyHostRequest {
    Spawn { config: LocalConfig, size: TerminalSize },
    Attach { session_id: LocalPtySessionId, size: TerminalSize },
    Resize { session_id: LocalPtySessionId, size: TerminalSize },
    Input { session_id: LocalPtySessionId, data: Vec<u8> },
    Close { session_id: LocalPtySessionId, mode: LocalPtyCloseMode },
    Query { session_id: LocalPtySessionId },
    KillDetached { session_ids: Vec<LocalPtySessionId> },
}

pub enum LocalPtyHostEvent {
    Spawned { session_id: LocalPtySessionId, child_pid: Option<u32> },
    Attached { session_id: LocalPtySessionId, child_pid: Option<u32> },
    Output { session_id: LocalPtySessionId, data: Vec<u8> },
    Exited { session_id: LocalPtySessionId, exit_code: i32 },
    Error { session_id: Option<LocalPtySessionId>, message: String },
}
```

**Step 2: 明确运行时 socket / pipe 路径规则**

抽出固定的 runtime endpoint 解析函数，避免把路径拼接散落在 `main` 和 `terminal` 两边。

**Step 3: 在 `TerminalBackend` 中引入关闭模式**

把现在的 `shutdown()` 扩成可表达 kill / detach 的接口，例如：

```rust
pub trait TerminalBackend: Send {
    fn write(&self, data: Vec<u8>);
    fn resize(&self, size: TerminalSize);
    fn close(&self, mode: TerminalCloseMode);
}
```

**Step 4: 为协议层补纯单测**

至少覆盖：
- close mode roundtrip
- invalid session id rejection
- runtime endpoint resolve
- `KillDetached` 空列表容错

**Step 5: 跑协议层定向测试**

Run: `cargo test -p terminal local_pty_protocol --lib`
Expected: 新协议单测全部通过。

### Task 2: 实现本地 PTY host 子进程入口

**Files:**
- Create: `crates/terminal/src/local_pty_host.rs`
- Create: `crates/terminal/src/local_pty_host_unix.rs`
- Create: `crates/terminal/src/local_pty_host_windows.rs`
- Modify: `crates/terminal/src/lib.rs`
- Modify: `crates/terminal/Cargo.toml`
- Modify: `main/src/main.rs`

**Step 1: 新增 host 入口函数**

在 `terminal` crate 暴露类似：

```rust
pub async fn run_local_pty_host() -> anyhow::Result<()>;
```

`main/src/main.rs` 在 GPUI 初始化前优先分发：

```rust
if std::env::args().any(|arg| arg == "--local-pty-host") {
    return terminal::run_local_pty_host();
}
```

**Step 2: 使用 `portable-pty` 实现 host 侧 session registry**

每个 session 至少维护：
- `session_id`
- `pty master`
- `child pid`
- `attached_client`
- `last_detached_at`
- `working_dir_at_spawn`

**Step 3: 在 host 侧实现 detach TTL 清理**

规则建议：
- attached client 断开时，将 session 标记为 detached
- detached 超过 `10 min` 自动 kill
- shell 自己退出时立刻清理 session

**Step 4: 实现 Unix / Windows 传输层**

Unix 使用 `tokio::net::UnixListener`；Windows 使用 named pipe。不要把平台分支塞进一个超大文件。

**Step 5: 增加 host 进程自举和探活**

UI 端如果发现 host 未运行，需要自动拉起；如果 endpoint 文件存在但不可连接，需要清理陈旧 endpoint 后重拉。

**Step 6: 跑 compile-only 验证**

Run: `cargo check -p terminal`
Expected: host 侧新增模块在当前平台可编译。

### Task 3: 把本地终端后端改成 host client 模式

**Files:**
- Create: `crates/terminal/src/local_pty_client.rs`
- Modify: `crates/terminal/src/pty_backend.rs`
- Modify: `crates/terminal/src/terminal.rs`
- Test: `crates/terminal/src/terminal.rs`

**Step 1: 为 UI 侧新增 host client**

client 职责：
- 确保 host 存活
- 发起 `Spawn` / `Attach`
- 持续接收 `Output` / `Exited`
- 转发 `Input` / `Resize` / `Close`

**Step 2: 保持 `Terminal` 的渲染模型不变**

UI 侧继续使用现有 `Term<GpuiEventProxy>`。host 返回的原始字节流直接喂给：

```rust
let mut processor: Processor<StdSyncHandler> = Processor::new();
processor.advance(&mut *term.lock(), &bytes);
```

不要在 host 内重复维护第二份 `Term`。

**Step 3: 让 `GpuiEventProxy` 的 `PtyWrite` 回写继续生效**

`PtyWriteBack` 需要支持把 DA / color query / text area size 请求经由 host client 回写给 PTY。

**Step 4: 在 `LocalPtyBackend` 上暴露 session identity**

至少要能拿到：
- `session_id`
- `child_pid`
- `is_hosted`

供恢复快照和关闭流程使用。

**Step 5: 保留一个可控 fallback 开关**

第一版建议加环境变量或内部开关，让 host 路径出问题时可以临时回退旧实现，便于灰度和问题定位。

**Step 6: 为 client/backend 补回归测试**

至少覆盖：
- spawn 成功后能拿到 `session_id`
- attach 不存在 session 返回可预期错误
- `Close(Kill)` 与 `Close(Detach)` 走不同分支
- `Exited` 事件能正确透传到 `Terminal`

**Step 7: 跑定向验证**

Run: `cargo test -p terminal local_pty --lib`
Expected: client / backend 新增测试通过。

### Task 4: 在恢复快照中持久化 live PTY 身份

**Files:**
- Modify: `crates/core/src/connection_restore.rs`
- Modify: `crates/terminal/src/terminal.rs`
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `main/src/home/home_tabs.rs`

**Step 1: 扩展 `LocalTerminalRestoreState`**

新增字段建议：

```rust
pub struct LocalTerminalRestoreState {
    pub working_dir: Option<String>,
    pub buffer_content: Option<String>,
    pub pty_session_id: Option<String>,
    pub prefer_live_restore: Option<bool>,
    // existing fields...
}
```

**Step 2: `dump()` 只在 hosted local terminal 下写入 `pty_session_id`**

纯 buffer 恢复路径仍保留，确保老快照和 live attach 共存。

**Step 3: 恢复时先 attach，失败再 fallback**

`TerminalView::new_restored_local_with_index(...)` 需要改成：
1. 若 `pty_session_id` 存在，先尝试 attach；
2. attach 成功时不再回放 `buffer_content`；
3. attach 失败时回退到当前的 `buffer_content + HISTORY_RESTORED_BANNER + new shell` 路径。

**Step 4: 补 payload 兼容测试**

Run: `cargo test -p one-core connection_restore --lib`
Expected: 新字段 roundtrip 通过，旧版不带 `pty_session_id` 的快照仍能恢复。

### Task 5: 把“关闭 / 退出 / 跳过恢复”三种生命周期分清

**Files:**
- Modify: `crates/terminal/src/types.rs`
- Modify: `crates/terminal/src/terminal.rs`
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `main/src/onetcli_app.rs`
- Modify: `main/src/home_tab.rs`
- Modify: `main/src/connection_restore.rs`

**Step 1: 手动关闭标签页时继续 kill**

`TerminalView::try_close()` / `force_close()` 仍应关闭 PTY，而不是 detach。

**Step 2: 应用整体退出时改为 detach**

在 app quit 流程中增加明确的 close context，确保本地 hosted PTY 在窗口销毁时执行 `Close(Detach)`，而不是当前无条件 `shutdown()`。

**Step 3: 恢复弹窗里用户显式“跳过”时清理 detached PTY**

如果某个恢复项持有 `pty_session_id` 但用户选择不恢复，应向 host 发送 `KillDetached`，避免孤儿 shell 残留到 TTL 超时。

**Step 4: 恢复成功后清理一次性 detached 标记**

attach 成功后，host 端要把 session 从 detached 状态切回 attached；UI 端保存新快照时仍写同一 `pty_session_id`。

**Step 5: 为 close semantics 补测试**

至少覆盖：
- tab 手动关闭 -> kill
- app quit -> detach
- skip restore -> kill detached

### Task 6: 增加恢复前探活与失败降级

**Files:**
- Modify: `main/src/connection_restore.rs`
- Modify: `main/src/home_tab.rs`
- Modify: `main/src/home/home_tabs.rs`
- Modify: `crates/terminal/src/local_pty_client.rs`

**Step 1: 首页加载恢复快照时做 live session 探活**

为每个带 `pty_session_id` 的本地终端恢复项做 lightweight `Query`：
- alive -> 标记为可 live restore
- missing -> 直接按普通 buffer 恢复项展示

**Step 2: attach 失败时降级而不是整项失败**

即使 host session 消失，也不能让本地终端恢复项直接丢失；必须自动退回当前 buffer replay 分支。

**Step 3: 把错误留到日志，不打断恢复批次**

一组恢复项中某个 live attach 失败，不应阻塞其它 SSH / Database / LocalTerminal 恢复。

**Step 4: 进行恢复链路手工冒烟**

Manual:
1. 打开本地终端，运行 `sleep 60`
2. 退出应用
3. 在 TTL 内重启应用
4. 在恢复弹窗中选择该终端
Expected: 终端恢复后仍连接到原会话，`sleep` 结束后 shell 继续可交互。

### Task 7: 平台验证、回归验证与发布门禁

**Files:**
- Modify: `docs/plans/2026-04-16-local-pty-live-restore.md`

**Step 1: Linux / macOS 先做全链路验证**

Run: `cargo check -p main --bin onetcli`
Expected: 主程序可编译。

Run: `cargo check -p terminal`
Expected: `terminal` crate 可编译。

Run: `cargo test -p one-core connection_restore --lib`
Expected: 恢复快照相关测试通过。

Run: `cargo test -p one-core tab_persistence --lib`
Expected: 标签持久化测试通过。

**Step 2: Windows 单独验证 named pipe 分支**

至少确认：
- host 可拉起
- spawn / attach / resize / kill 命令都可往返
- ConPTY 环境下不会吞掉输入或重复回显

**Step 3: 输出最终风险清单**

必须显式记录：
- broker 崩溃时的降级行为
- detached shell TTL 策略
- attach 失败时的数据丢失窗口
- Windows 命名管道实现差异

**Step 4: 分阶段提交**

建议提交拆分：
1. `test: unblock terminal crate unit tests`
2. `feat: add local pty host protocol and entrypoint`
3. `feat: move local terminal backend to hosted pty client`
4. `feat: persist local pty session ids for live restore`
5. `feat: detach local pty sessions on app quit`

## 推荐执行顺序

1. 先完成 Task 0 和 Task 1，不要一开始就改 UI 恢复流程。
2. Task 2 和 Task 3 做完后，先在隐藏开关下完成“新开本地终端可正常使用”。
3. 确认 hosted local terminal 稳定后，再做 Task 4 到 Task 6 的恢复链路接线。
4. Windows 分支如果明显拖慢主线，可以在 Linux/macOS 落稳后再启用。

## 关键风险

1. 当前本地终端实现强耦合 `alacritty_terminal` 事件循环；如果 host client 的字节流回放无法完整兼容 `PtyWrite` / `ColorRequest` / `TextAreaSizeRequest`，需要及时退回到“UI 侧自定义 read loop + `GpuiEventProxy` 回写”备选方案。
2. 如果不区分 `Kill` 和 `Detach`，应用退出时仍会杀掉 PTY，功能表面完成但语义错误。
3. 如果不在“跳过恢复”路径清理 detached PTY，会留下不可见孤儿 shell。
4. 如果 live attach 失败时不回退到 `buffer_content`，用户会觉得恢复功能退化。
