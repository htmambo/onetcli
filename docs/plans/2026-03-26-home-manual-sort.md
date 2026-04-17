# 首页手动排序 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为首页连接列表新增“手动排序”模式，支持拖拽调整工作区顺序和同一工作区内连接顺序，并将结果持久化到本地与云端同步数据。

**Architecture:** 在 `Workspace` 与 `StoredConnection` 上新增 `sort_order` 字段，将“手动排序”并入现有 `ConnectionListSortField` 枚举。首页在 `Manual` 模式下改为按 `sort_order` 渲染，并仅在该模式启用同层级拖拽；拖拽落下后批量更新当前组顺序，复用现有仓库和同步链路落库与同步。

**Tech Stack:** Rust、GPUI、SQLite、现有 storage repository、cloud_sync、首页 HomePage 渲染体系

---

### Task 1: 固化上下文与设计边界

**Files:**
- Modify: `.claude/operations-log.md`
- Create: `.claude/context-summary-home-manual-sort.md`
- Create: `docs/plans/2026-03-26-home-manual-sort-design.md`

**Step 1: 记录根因与范围**

写明当前首页只有名称 / 创建时间 / 更新时间三种排序，且实体与同步数据都没有顺序字段。

**Step 2: 固化边界**

明确“仅支持工作区之间重排、同一工作区内连接重排、未分配区内重排；不支持跨工作区改变归属”。

### Task 2: 扩展排序枚举与模型字段

**Files:**
- Modify: `main/src/setting_tab.rs`
- Modify: `crates/core/src/storage/models.rs`

**Step 1: 扩展排序字段枚举**

给 `ConnectionListSortField` 新增 `Manual` 枚举值，并保持序列化兼容。

**Step 2: 扩展实体模型**

给 `Workspace` 与 `StoredConnection` 新增 `sort_order` 字段，并补默认值语义。

**Step 3: 更新依赖构造代码**

修复所有手写构造 `Workspace` / `StoredConnection` 的测试或辅助代码，确保新字段完整。

### Task 3: 编写 migration

**Files:**
- Create: `crates/migrations/20260326000003_home_manual_sort.sql`
- Modify: `crates/core/src/storage/migration.rs`

**Step 1: 给表加列**

为 `workspaces` / `connections` 增加 `sort_order INTEGER NOT NULL DEFAULT 0`。

**Step 2: 回填旧数据顺序**

按当前默认显示顺序为：
- 工作区
- 每个工作区内连接
- 未分配连接

分别生成稳定的 `sort_order`。

**Step 3: 注册 migration**

把新 migration 加入 `MIGRATIONS` 常量。

### Task 4: 扩展 repository 读写与批量重排接口

**Files:**
- Modify: `crates/core/src/storage/repository.rs`

**Step 1: 扩展 row mapping 与 SQL**

让连接和工作区的查询、插入、更新、云端回写都包含 `sort_order`。

**Step 2: 新增批量重排方法**

新增事务化方法，例如：
- `WorkspaceRepository::reorder(...)`
- `ConnectionRepository::reorder_within_workspace(...)`

要求只更新目标组，不影响其他组。

**Step 3: 补仓库测试**

验证批量重排后读取顺序正确，且非目标组不受影响。

### Task 5: 扩展云同步数据结构

**Files:**
- Modify: `crates/core/src/cloud_sync/models.rs`
- Modify: `crates/core/src/cloud_sync/service.rs`
- Modify: `crates/core/src/cloud_sync/workspace_sync.rs`
- Modify: `crates/core/src/cloud_sync/connection_sync.rs`

**Step 1: 扩展 plain data**

给 `WorkspacePlainData` 与 `ConnectionPlainData` 增加 `sort_order` 字段。

**Step 2: 扩展上传路径**

上传本地实体时把 `sort_order` 放进加密 blob。

**Step 3: 扩展下载与云端覆盖本地路径**

从云端恢复 `sort_order`，确保其他设备能看到同样顺序。

**Step 4: 补同步测试**

验证顺序字段的序列化、反序列化和本地恢复。

### Task 6: 把 Manual 排序接入首页

**Files:**
- Modify: `main/src/home_tab.rs`
- Modify: `main/locales/main.yml`

**Step 1: 扩展排序菜单**

在首页排序字段菜单中新增“手动排序”。

**Step 2: 调整排序标签与按钮行为**

当排序字段为 `Manual` 时：
- 排序字段标签显示手动排序
- 升降序按钮禁用或隐藏

**Step 3: 扩展比较逻辑**

让 `compare_workspaces(...)` / `compare_connections(...)` 支持 `Manual` 分支。

**Step 4: 补排序测试**

新增单测验证 `Manual` 模式按 `sort_order` 生效。

### Task 7: 接工作区拖拽重排

**Files:**
- Modify: `main/src/home_tab.rs`

**Step 1: 定义拖拽 payload**

定义工作区拖拽数据结构，只包含当前实体和来源信息。

**Step 2: 给工作区 header 加拖拽与 drop**

只在 `Manual` 模式启用。

**Step 3: 实现重排逻辑**

落下后：
- 更新内存中的工作区顺序
- 重算该组 `sort_order`
- 调用仓库批量更新
- 触发现有同步

**Step 4: 处理失败回滚**

如果落库失败，恢复原顺序并提示错误。

### Task 8: 接连接拖拽重排

**Files:**
- Modify: `main/src/home_tab.rs`

**Step 1: 定义连接拖拽 payload**

包含连接 ID、来源工作区 ID。

**Step 2: 给连接列表项和卡片加拖拽**

列表模式与卡片模式都接入。

**Step 3: 限制仅同组重排**

只有来源工作区与目标工作区一致时才允许落下重排；否则忽略。

**Step 4: 持久化顺序**

重算该组连接 `sort_order`，批量更新仓库并触发同步。

### Task 9: 做回归验证

**Files:**
- Create: `.claude/verification-report-home-manual-sort.md`
- Modify: `.claude/operations-log.md`

**Step 1: 运行针对性测试**

至少覆盖：
- `cargo test -p one-core ...`
- `cargo test -p main ...`
- `cargo check -p one-core -p main`

**Step 2: 手工验证**

验证：
- 非手动模式下无拖拽
- 手动模式下工作区可拖拽
- 手动模式下同组连接可拖拽
- 跨工作区拖放无效
- 重启后顺序保留
- 同步后另一端顺序一致

**Step 3: 写验证报告**

记录通过项、限制项和风险项。

### Task 10: 提交策略

**Files:**
- None

**Step 1: 当前会话不自动提交**

遵循当前仓库协作约束，本轮实现完成后不自动 `git commit`，由用户明确要求后再提交。
