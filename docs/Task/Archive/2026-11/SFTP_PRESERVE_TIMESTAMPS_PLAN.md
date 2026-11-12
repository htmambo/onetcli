# SFTP 下载保留远程时间戳

**Status**: ✅ Completed & Accepted (completion: 2026-11-12，archived: 2026-11-12)
**Created**: 2026-11-12
**Creator**: Claude Code
**Branch**: refactor/remove-external-db-drivers

## 1. 目标与背景

使用 SFTP 下载远程文件时，把远程文件的修改时间（mtime）与访问时间（atime）写到本地副本上，避免本地文件时间统一变成"下载时刻"。覆盖单文件下载与递归目录下载两条路径。

## 2. 边界（非目标）

1. **不保留创建时间（birth time）**：OpenSSH `sftp-server` 只实现 SFTP v3，协议不暴露 create_time；Linux 本地也无法设置创建时间。跨平台不可行，明确不承诺。
2. **不做上传方向**（本地时间写回远端）：本次仅处理下载。
3. **不加设置开关**：下载即保留时间，行为与 `scp -p` / `rsync -t` 的预期一致，UI 不变。
4. 不改进度回调、传输任务结构与错误分类。

## 3. 现状分析（代码事实）

- `crates/sftp/src/russh_impl.rs:903` `download_with_progress` 已调用 `sftp.metadata(remote_path)`，`atime`/`mtime` 已在手，无需额外往返。
- `serial_download_file`（`:594`）、`pipelined_download`（`:302`）、`pipelined_download_with_file_progress`（`:461`）写完只做 `flush` + `sync_all`，本地 mtime 为下载时刻。
- `download_dir_with_progress`（`:1293`）用 `create_dir_all` 建目录、逐个下载文件，目录时间从未恢复。
- `list_dir`（`:854`）/ `list_dir_recursive`（`:1255`）读取的 `FileAttributes` 含 `atime`/`mtime`，但 `FileEntry` 只有 `modified: SystemTime`。
- `filetime 0.2.25` 已在 `Cargo.lock` 中（`notify`/`tar` 传递依赖），加为直接依赖不引入新传递依赖。
- `crates/sftp` 为 `publish = false` 的内部 crate，`FileEntry` 无序列化、无 IPC 使用；新增字段只影响 crate 内两个构造点。
- 本地消费方审计：应用内没有基于"已下载文件 mtime"的逻辑（`sftp_view` 只做展示，`main/src/update/download.rs` 的 mtime 属更新包缓存，与 SFTP 无关）。

## 4. 方案

### 4.1 新增 `crates/sftp/src/file_times.rs`

- `RemoteTimes { atime: Option<SystemTime>, mtime: Option<SystemTime> }`
  - `from_sftp_attrs(atime: Option<u32>, mtime: Option<u32>)`：SFTP v3 秒级时间转换，缺失保持 `None`。
  - `from_file_entry(&FileEntry)`：`mtime = Some(modified)`，`atime` 保持 `Option`。
  - `apply_to_path(&Path)`：`mtime` 缺失直接跳过；`atime` 存在时用 `set_file_times`，否则用 `set_file_mtime`；失败只 `tracing::warn`（i18n key），绝不让传输失败。
- `apply_dir_times(Vec<(PathBuf, RemoteTimes)>)`：按路径深度倒序应用，保证父目录时间最后落定。真正的正确性前提是"所有子项写完后再设置"，倒序是保守兜底。

### 4.2 `FileEntry` 增加 `atime: Option<SystemTime>`

- 两个构造点（`list_dir`、`list_dir_recursive`）填充；`atime` 缺失不伪造，与 `RemoteTimes` 语义一致。

### 4.3 下载链路穿透

- 单文件：`download_with_progress` 由 `metadata` 构造 `RemoteTimes`，传给 `serial_download_file` / `pipelined_download`；在 `flush` + `sync_all`、文件句柄关闭后应用。
- 目录：每个文件完成后应用自身时间；大文件透传给 `pipelined_download_with_file_progress`；子目录时间在全部文件结束后统一应用，根目录时间通过一次 `metadata(remote_path)` 获取并最后应用。
- 空目录：`list_dir_recursive` 已把目录条目收进结果，目录创建阶段照常建出并恢复时间。

### 4.4 依赖与文案

- 根 `Cargo.toml` 的 `[workspace.dependencies]` 增加 `filetime = "0.2"`，`crates/sftp/Cargo.toml` 引用 workspace 版本。
- `crates/sftp/locales/sftp.yml` 增加 `Sftp.apply_times_failed`（en / zh-CN / zh-HK）。

### 4.5 取消、失败与边界语义（评审 F1/F2/F3 决策）

- **取消/失败**：`download_dir_with_progress` 在任一 `ensure_not_cancelled` 或读写错误处直接返回 `Err`，目录时间只在函数正常走到末尾时应用。即"中断/部分下载"不会把目录标记为已完成时间。
- **mtime 缺失与哨兵 0**：`from_sftp_attrs` 把 `None` 和 `0` 都视为"服务端未提供"，跳过设置，不写入 1970。`list_dir` 的 `modified` 对哨兵 0 也回退为列表时刻（与缺失一致）。
- **符号链接**：本地下载只创建普通文件/目录（`File::create` / `create_dir_all`），不会创建 symlink；若目标路径是用户已有的 symlink，现有实现本来就会跟随写入目标文件，`filetime` 跟随同一目标设置时间，不引入新的覆盖风险。
- **目录时间应用顺序**：依赖"全部子项写入完成"这一前置条件；目录之间顺序实际无关，深度倒序仅作防御。

## 5. 子任务

1. ✅ 新增 `file_times.rs` 与单元测试（转换、单路径应用、失败不 panic、递归目录应用）
2. ✅ `FileEntry` 增加 `atime` 字段并填充两个构造点
3. ✅ 单文件下载链路（serial + pipeline）应用时间戳
4. ✅ 目录下载链路（文件 + 子目录 + 根目录）应用时间戳
5. ✅ 依赖与 i18n 文案接入
6. ✅ 验证：`cargo test -p sftp` 13/13 通过；`cargo fmt --check` 干净；`cargo check -p sftp_view` / `cargo check -p main` 通过
7. ✅ 外部评审（code）完成；文档归档待用户验收

## 6. 验收标准

- 单文件下载后本地 mtime 等于远程 mtime（秒级）；atime 在服务端提供时一并恢复。
- 递归目录下载后，文件与各级目录（含空目录、根目录）的 mtime 均恢复。
- 服务端未提供 mtime 时行为与现状一致，不报错。
- 设置时间失败（路径不存在/权限不足/不支持的文件系统）只产生 warn 日志，文件内容与传输结果不受影响。
- `cargo test -p sftp` 全绿；`cargo clippy -p sftp -- -D warnings` 无新增告警；`cargo fmt --check` 干净。

## 7. 风险与缓解

- **精度损失**：SFTP v3 只有秒级，纳秒归零；FAT/exFAT 可能按 2 秒粒度截断。属预期行为。
- **atime 语义弱**：relatime 挂载下读取会更新 atime，恢复 atime 仅为尽力而为；mtime 是关键指标。
- **平台差异**：Windows 无法用 std 打开目录句柄设置时间，故选用 `filetime`（`FILE_FLAG_BACKUP_SEMANTICS`）。本次不新增 Windows CI 矩阵，依赖 filetime 上游测试 + 单元测试覆盖本地逻辑。
- **行为变更**：下载文件时间不再是"现在"，可能影响外部备份/同步工具的判断；已审计应用内无依赖该 mtime 的逻辑。
- **回滚**：纯行为变更、无配置开关，回滚即 revert。

## 8. 外部评审记录

- Requirement 评审（2026-11-12）：**APPROVED**。建议已吸收：`FileEntry.atime` 使用 `Option` 避免伪造；补目录时间应用的单测；本地 mtime 消费方已完成审计；`FileEntry` 确认仅 workspace 内部使用（`publish = false`、无 serde）。
- Plan 评审（2026-11-12）：**APPROVED**（附前置确认项）。F1 取消/失败语义、F2 mtime 缺失与 0 哨兵、F3 symlink 策略已写入 §4.5；F4 Windows 实机验证、F5 参数穿透形态、F6 doc-comment、F7 临时目录管理、F8 字段类型已在实现中处理。
- Code 评审（2026-11-12）：**APPROVED**（条件为核实 F0，已核实为摘录省略导致的误报：真实代码为 `let result = ...await; if result.is_err() { return result; }`，错误正常传播）。处理情况：
  - F1 语义不一致：`from_file_entry`、`FileEntry::modified` 增加文档注释固化"未知 → 列表时刻"约定，并补 `from_file_entry_takes_entry_times` 测试。
  - F4 测试缺口：补 `apply_with_both_times_sets_mtime`、`apply_with_only_atime_is_noop`。
  - F5 哨兵冲突：时间转换溢出回退从 `UNIX_EPOCH` 改为 `SystemTime::now()`（理论不可达）。
  - F2/F3/F6/F7/F8：F2 单次 syscall 与既有 `create_dir_all` 同步调用一致，暂不引入 `spawn_blocking`；F3 `FileEntry` 无 serde、仅 workspace 内部使用；F6 `dirs` 不含根条目，无重复；F7 属既有 B5 校验的相邻问题（`..` 已在 `download_dir_with_progress` 拦截），不在本次范围；F8 由 warn-only 策略兜底。

## 9. 验收记录

- 自动化验证（2026-11-12）：
  - `cargo test -p sftp`：13 passed（含时间戳转换、单文件应用、no-op、目录层级）
  - `cargo check -p sftp_view`、`cargo check -p main`：Finished
  - `cargo fmt --check`：干净
  - `cargo clippy -p sftp --no-deps`：仅 1 条既有告警 `russh_impl.rs:227`（`String::from_utf8(...).unwrap()`，本次未触碰），无新增告警；`-D warnings` 全仓模式会被 `ssh` crate 既有告警阻断，与本次改动无关
- 手工验收（2026-11-12）：用户实测下载文件 `mtime` 与远程一致，通过。
- 说明：创建时间（birth time）不在 SFTP v3 协议内，linux 本地也无法设置，本功能不包含。
