# SSH GEX 最小组尺寸兼容修复

**状态**: ✅ Completed（完成于 2026-07-01）
**创建时间**: 2026-07-01
**影响范围**: `crates/ssh`（含 `crates/sftp` 透传）
**关联提交**: `dca6b674` — `fix(ssh): GEX 默认 min_group_size 下调到 2048 修复旧服务端 KEX 失败`

## 背景

用户反馈连接远程主机时日志反复出现：

```
WARN russh::client::kex: DH prime size (2048 bits) not within requested range
```

且连接直接失败。日志连续出现 4 次（4 台目标主机），与"无法连接远程主机"的描述一致。

## 根因

`russh-0.60.3` 的 GEX 协商（`russh-0.60.3/src/client/kex.rs:207-215`）会校验服务端返回的 DH 素数位数：

```rust
if group.bit_size() < self.config.gex.min_group_size
    || group.bit_size() > self.config.gex.max_group_size
{
    warn!("DH prime size ({} bits) not within requested range", group.bit_size());
    return Err(Error::KexInit);
}
```

而 `GexParams::default()`（`russh-0.60.3/src/client/mod.rs:1787-1795`）是：

```rust
min_group_size: 3072,
preferred_group_size: 8192,
max_group_size: 8192,
```

`Preferred::default()` 的 kex 列表默认包含 `DH_GEX_SHA256`（`russh-0.60.3/src/negotiation.rs:107`）。当远端服务器在 GEX 协商中下发 2048-bit 的素数（RFC 2412 Group 2 / 多数 OpenSSH 默认行为），客户端会因 `2048 < 3072` 立刻 `Err(Error::KexInit)` —— **warn 等价于连接失败**。

## 项目侧的次级问题

`crates/ssh/src/ssh.rs` 早就准备了 `enable_legacy_kex` 开关与 `build_client_config`（`ssh.rs:230-247`），把 `gex` 调到 `GexParams::new(2048, 4096, 8192)`。但 `RusshClient::connect`（`ssh.rs:1498-1506`）的"直接连接 / 跳板 / 代理"三条路径都用了**裸的 `Config::default()`**，没有套用 `build_client_config`：

```rust
let russh_config = Arc::new(client::Config {
    inactivity_timeout: ...,
    keepalive_interval: ...,
    keepalive_max: ...,
    ..<_>::default()   //  ← 此处未应用 gex 与 preferred
});
```

——即只有显式打开 `enable_legacy_kex=true` 才能下发兼容 GEX。这是疏漏。

## 修复目标

1. `RusshClient::connect` 统一复用 `build_client_config(&config)`，与"遗留兼容模式"使用同一套 kex/cipher/gex 调整。
2. 默认 GEX 范围下调为 `(2048, 4096, 8192)`（含 `enable_legacy_kex=false` 路径），与 RFC 4253 / OpenSSH 客户端默认一致。
3. 不引入新行为给 `enable_legacy_kex=true`：仍会把 `DH_G14_SHA1 / DH_GEX_SHA1 / DH_G1_SHA1` 与 CBC 密码推到 preferred 头部。

## 子任务

- [ ] **Sub-task 1**：`RusshClient::connect` 改用 `build_client_config(&config)`。
  - 涉及：`crates/ssh/src/ssh.rs:1498-1506`（直接连接分支主 Config 构造点）
  - 三处 use 点的 `Config { inactivity_timeout, keepalive_interval, keepalive_max, ..<_>::default() }` 全部替换为 `build_client_config(&config)`（含 `enable_legacy_kex` 是否开启的判断由 `build_client_config` 内部完成）。
  - 跳板 / 代理分支内的 `Config` 也需同步调整（参 `ssh.rs:1498-1506`、`1565-1575`、跳板机 `client::connect_stream(russh_config, ...)`）。
  - 注：`russh_config` 原本是 `Arc<client::Config>`，`build_client_config` 返回 `client::Config`（未包 Arc），调用处改为 `Arc::new(build_client_config(&config))`。

- [ ] **Sub-task 2**：默认 `gex` 改为 `GexParams::new(2048, 4096, 8192)`。
  - 改 `build_client_config`（`ssh.rs:230-247`）的 `if enable_legacy_kex` 分支：把 GEX 参数应用到**两条路径**（不再仅在 legacy 模式才下调）。
  - 改后逻辑：
    - `gex` 一律 `GexParams::new(2048, 4096, 8192)`，保证默认也能接受 2048-bit 服务端组。
    - `enable_legacy_kex` 仅控制是否向 `preferred.kex` / `preferred.cipher` 追加 legacy 算法。

- [ ] **Sub-task 3**：验证 `cargo build -p ssh` 与 `cargo test -p ssh`。

## 风险评估

| 风险点 | 评估 | 缓解 |
|---|---|---|
| `Preferred::default()` 中 `DH_GEX_SHA256` 仍存在，服务端若只支持 `DH_GEX_SHA1`（更老）可能仍失败 | 中 | 旧服务端本就需要 `enable_legacy_kex=true`；本任务不扩大 legacy 算法面 |
| 把 `gex` 放宽到 2048 会降低对超大组（8192）失败时的兜底 | 低 | 协商会从 preferred=4096 开始；`max_group_size` 仍为 8192 |
| `RusshClient::connect` 三处都改，可能漏改 | 中 | 用 `replace_all` 不适用，靠阅读 1500-1620 整段一次性覆盖 |
| SFTP 路径间接受影响 | 低 | SFTP 走 `crates/ssh` 透传 `RusshClient`，自动获益 |

## 验收标准

1. `cargo build -p ssh -p sftp` 通过。
2. `cargo test -p ssh` 现有测试通过。
3. 对支持 2048-bit GEX 的远端（旧 OpenSSH / 多数 Linux 服务器）不再出现 `DH prime size (2048 bits) not within requested range` 警告，连接成功。
4. `enable_legacy_kex=true` 行为不变（legacy kex/cipher 仍追加到 preferred）。
5. `enable_legacy_kex=false` 行为：preferred 列表与 russh 0.60.3 默认一致，但 gex 接受 2048。

## 实施顺序

1. 改 `build_client_config`（Sub-task 2）—— 单点修改，验证编译。
2. 改 `RusshClient::connect` 三处 Config 构造（Sub-task 1）—— 同步改跳板 / 代理 / 直接三分支。
3. 跑 `cargo test -p ssh`（Sub-task 3）。
4. 提交（commit 模板见 `~/.claude/COMMIT_TEMPLATE.md`，含 OMC trailer）。
5. 归档：移动到 `docs/Task/Archive/2026-07/`，更新 `docs/Task/README.md`。

## 备注

- russh 0.60.3 仍处于 `min_group_size < 2048` 校验（`client/mod.rs:1707-1712`），2048 是最低合法值。
- 远端如果是仅 1024-bit 的 OpenSSH（极少见，几乎都是 14.3+），仍需 `enable_legacy_kex=true`。
