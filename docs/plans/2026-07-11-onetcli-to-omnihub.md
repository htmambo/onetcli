# OnetCli → OmniHub Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将产品名、二进制、配置目录与打包标识从 OnetCli/onetcli 统一切换为 OmniHub/omnihub，并保留旧数据迁移与密钥兼容。

**Architecture:** 分四批推进：P0 构建壳层与模块改名 → P1 运行时路径/IPC/环境变量兼容迁移 → P2 UI/业务文案 → P3 对外文档。仓库 URL 与内部 crate 名本轮不动；密钥派生字符串保持旧值。

**Tech Stack:** Rust workspace、GPUI 桌面应用、shell/PowerShell 打包脚本、macOS/Linux/Windows 资源清单。

---

### Task 1: 配置目录与数据目录迁移

**Files:**
- Modify: `crates/core/src/storage/runtime_paths.rs`
- Modify: `crates/core/src/key_storage.rs`

**Steps:**
1. 将 `get_config_dir()` 目标改为 `omnihub`，并实现 `one-hub`/`onetcli` → `omnihub` 迁移
2. DB 名改为 `omnihub.db`，兼容旧 `one-hub.db`
3. 安装主题路径改为 `/usr/share/omnihub/themes`
4. `key_storage` 数据目录改为 `omnihub`，保留 salt/info 旧值，并迁移旧 `one-hub` 数据目录

### Task 2: 二进制与应用模块改名

**Files:**
- Modify: `main/Cargo.toml`
- Rename: `main/src/onetcli_app/` → `main/src/omnihub_app/`
- Modify: `main/src/main.rs` 及所有 `OnetCliApp` 引用

### Task 3: 平台资源与打包脚本

**Files:**
- Modify/Rename: `resources/**`
- Modify: `script/**`, `pkg/arch/PKGBUILD`, `main/build.rs`, `ONETCLI_LICENSE`

### Task 4: IPC / PTY / Shell 集成 / 更新器

**Files:**
- Modify: `crates/ipc/src/socket.rs`
- Modify: `crates/terminal/src/*`
- Modify: `main/src/update/*`
- Modify: `crates/core/src/config.rs`, `crates/core/build.rs`

### Task 5: UI/业务文案与文档

**Files:**
- Modify: about/home/locales/cloud/llm 等
- Modify: README*/DONATE*/SPONSOR*/站点 docs

### Task 6: 验证

1. `cargo check -p one-core`
2. `cargo check -p main`
3. 针对路径迁移与关键 rename 点做定向测试/搜索确认
