# Upstream Dev 吸收实施计划

> **分支:** `integration/upstream-dev-absorb-2026-04-16`
> **基线:** `feat/tabby-terminal-restore`
> **上游目标:** `upstream/dev` @ `6a7f1ff0`

**Goal:** 在不提交 git commit 的前提下，将 `upstream/dev` 相对共同基线 `29b0692f` 的 25 个提交吸收到当前功能分支，优先保留当前分支的本地终端恢复语义，同时接入上游的更新系统、终端历史/自动补全、代理设置、MySQL 修复和版本文档更新。

**Boundary:**
- 不执行 `git commit`、`git push`、`git rebase`
- 默认使用一次 `merge --no-commit` 吸收完整上游历史
- 若某些冲突区域不适合机械合并，则在冲突文件内做人工整合
- 本次不额外引入超出 `upstream/dev` 的新功能

**风险热点:**
- `crates/terminal/**` 与 `crates/terminal_view/**`：当前分支和上游都重改
- `main/src/setting_tab.rs`：上游代理设置、更新开关、终端设置全局化都落在这里
- `main/src/onetcli_app.rs`：本地终端恢复逻辑与上游移除 tab persistence 存在设计冲突
- `main/src/update.rs` -> `main/src/update/*`：上游把单文件更新模块拆成目录
- `crates/core/src/lib.rs`：上游移除 `tab_persistence` 导出

---

## 功能包拆分

### 包 A：更新系统

提交：
- `5266b586`
- `984f1c47`
- `869c6831`
- `07b9c6b4`
- `daf0e271`
- `c7cd3ef4`

目标：
- 接入应用内更新检查、下载、安装、校验与发布页跳转
- 保留当前分支已有行为，避免回退现有本地终端恢复逻辑

冲突策略：
- 优先保留上游模块化更新目录结构
- 若当前分支仍保留旧的 `main/src/update.rs` 入口，则重定向到上游 `mod.rs`

### 包 B：全局 HTTP 代理

提交：
- `0671bd6a`

目标：
- 接入全局 HTTP 代理设置及对应 UI

冲突策略：
- 在 `setting_tab` 中以功能块方式并入
- `auth` / `onetcli_app` 优先保留当前分支已有初始化流程，再补上代理设置接线

### 包 C：merge 带入的杂项能力

提交：
- `2fc5531f`
- `3266ca7e`

说明：
- 这两条 merge commit 已经包含在 `upstream/dev` 最终树中，不单独 cherry-pick
- 由整体 merge 自动吸收；冲突时只处理最终文件状态，不追求复现 merge 过程

### 包 D：赞助移除

提交：
- `f892457a`

目标：
- 移除 encourage 相关入口，改为 `SPONSOR.md`

冲突策略：
- 上游删除 `main/src/encourage.rs` 时，若当前分支未继续使用该模块，则直接接受删除

### 包 E：终端历史 / 自动补全 / SSH shell 集成 / 设置全局化

提交：
- `48e4e3f2`
- `41b02d98`
- `66ba7b9c`
- `685946f9`
- `e340f72f`
- `3e0bd0b5`
- `94bbe5f0`
- `2bc60ebb`
- `42c1df05`

目标：
- 接入上游终端历史、history prompt、shell integration、SSH shell 集成、自动补全全局开关与设置持久化
- 保留当前分支的 hosted pty / local pty live restore 方案

冲突策略：
- `crates/terminal/src/pty_backend.rs`、`crates/terminal/src/terminal.rs` 优先保留当前分支的本地 pty 宿主与恢复语义，再手工并入上游 history/autocomplete 逻辑
- `crates/terminal_view/src/view.rs` 以功能块整合，不回退任何当前分支关于恢复流程的调用
- `crates/terminal_view/src/settings.rs` 若为上游新增文件，直接吸收
- `main/src/setting_tab.rs` 保留当前分支已有设置布局，再补齐上游全局终端设置项

### 包 F：MySQL 表设计修复

提交：
- `48814220`

目标：
- 接入列变更检测、排序诊断日志与相关回归修复

冲突策略：
- 对 `db` / `db_view` 层改动尽量接受上游
- 若该提交混入 terminal 相关文件，则只保留与表设计修复直接相关的实际代码

### 包 G：tab persistence 移除

提交：
- `a328780d`

目标：
- 接受上游移除共享 tab persistence 的方向，但不能破坏当前分支的本地终端恢复能力

冲突策略：
- `crates/core/src/lib.rs` 可以接受移除导出
- `main/src/onetcli_app.rs` 需要人工判断：保留当前分支用于本地终端恢复的入口逻辑，不机械删除

### 包 H：版本与文档

提交：
- `b5bcab49`
- `084711eb`
- `a45b26be`
- `6a7f1ff0`

目标：
- 同步版本号、README 与文档

冲突策略：
- 接受 `0.2.5`
- README 以更完整的上游内容为主，如与当前分支新增说明冲突则人工合并

---

## 实施顺序

- [ ] Step 1: 运行 `git merge --no-commit upstream/dev`，获得完整冲突集合
- [ ] Step 2: 先处理删除/重命名类冲突：`main/src/update.rs`、`main/src/encourage.rs`、`crates/core/src/cloud_sync/supabase.rs`
- [ ] Step 3: 处理低耦合模块：README、版本、更新模块目录、代理设置
- [ ] Step 4: 处理数据库层：`crates/db/**`、`crates/db_view/**`
- [ ] Step 5: 处理终端热点：`crates/terminal/**`、`crates/terminal_view/**`
- [ ] Step 6: 处理 app glue：`main/src/setting_tab.rs`、`main/src/auth.rs`、`main/src/onetcli_app.rs`
- [ ] Step 7: 运行定向验证，至少覆盖编译入口或相关 crate
- [ ] Step 8: 输出当前吸收结果、剩余风险和建议的人工复核点

## 验证策略

- 首选：`cargo check`
- 若全量成本过高，则退化为：
  - `cargo check -p terminal`
  - `cargo check -p terminal_view`
  - `cargo check -p main`

## 当前状态

- [x] 已创建集成分支
- [ ] 尚未开始 merge
- [ ] 尚未进入冲突解决
- [ ] 尚未执行验证
