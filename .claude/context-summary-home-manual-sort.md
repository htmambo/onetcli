## 项目上下文摘要（home-manual-sort）
生成时间：2026-03-26 22:13:40 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:2899-3003`
  - 模式：首页连接列表排序完全由 `ConnectionListSortField` / `ConnectionListSortOrder` 驱动。
  - 可复用：`sort_connections_for_display(...)`、`connection_list_sort_field(...)`、`update_connection_list_preferences(...)`。
  - 需注意：当前只有名称、创建时间、更新时间三种排序字段，没有“手动排序”模式。

- **实现2**: `main/src/home_tab.rs:3114-3337`
  - 模式：首页先构造“工作区 + 该工作区连接列表”，再分别排序后渲染 section / collection。
  - 可复用：`render_workspace_view(...)`、`render_workspace_section(...)`、`render_connections_collection(...)`。
  - 需注意：工作区排序和连接排序都在这里统一执行，是手动排序切入点。

- **实现3**: `main/src/home_tab.rs:3943-4010`
  - 模式：`compare_workspaces(...)` / `compare_connections(...)` 负责所有排序比较逻辑。
  - 可复用：把 `Manual` 作为第四个分支并入现有排序体系。
  - 需注意：未分配工作区固定排最后，这个规则在手动排序下也要保留。

- **实现4**: `crates/core/src/storage/models.rs:694-738` 与 `crates/core/src/storage/models.rs:471-...`
  - 模式：`Workspace` 和 `StoredConnection` 是首页排序与同步的实体源。
  - 可复用：直接在实体上新增 `sort_order` 字段。
  - 需注意：当前实体没有任何“显示顺序”字段。

- **实现5**: `crates/core/src/storage/repository.rs:198-282` 与 `crates/core/src/storage/repository.rs:772-910`
  - 模式：连接和工作区仓库负责本地落库、云端回写、本地更新。
  - 可复用：`insert_from_cloud(...)`、`update_from_cloud(...)`、`Repository::update(...)`。
  - 需注意：SQL 语句当前没有 `sort_order` 列，需要 migration 和仓库层同步补齐。

- **实现6**: `crates/core/src/cloud_sync/models.rs:357-386` 与 `crates/core/src/cloud_sync/service.rs:353-417`
  - 模式：云同步采用实体明文结构 `ConnectionPlainData` / `WorkspacePlainData` 打包加密上传。
  - 可复用：把 `sort_order` 直接作为明文字段同步。
  - 需注意：如果不把顺序写入 plain data，本地拖拽结果无法同步到远端。

- **实现7**: `crates/ui/src/table/state.rs:1299-1324`、`crates/ui/src/dock/tab_panel.rs:755-771`
  - 模式：项目已有 `on_drag + drag_over + on_drop` 的拖拽交互范式。
  - 可复用：首页列表项和工作区头部也可以沿用这一套拖拽 API。
  - 需注意：需要自己定义拖拽 payload，并处理同组内重排高亮。

### 2. 已确认需求
- 首页连接列表新增第四种排序：手动排序。
- 手动排序允许：
  - 工作区之间拖拽重排。
  - 同一工作区内连接拖拽重排。
  - 未分配区内部连接拖拽重排。
- 手动排序不允许：
  - 跨工作区拖拽改变连接归属。
  - 把连接拖到别的工作区中。
- 手动排序结果需要：
  - 本地持久化。
  - 云端同步并在其他端恢复。

### 3. 数据与迁移结论
- 需要给 `workspaces` / `connections` 表新增 `sort_order`。
- 需要给 `Workspace` / `StoredConnection` 新增 `sort_order` 字段。
- migration 需要对旧数据回填默认顺序，避免升级后列表突然跳变。
- 建议按当前默认显示顺序回填：
  - 工作区：`updated_at DESC`
  - 每个工作区内连接：`updated_at DESC`
  - 未分配连接：`updated_at DESC`

### 4. 同步结论
- `WorkspacePlainData` 要新增 `sort_order`
- `ConnectionPlainData` 要新增 `sort_order`
- 上传、下载、远端覆盖本地都要带上顺序字段
- 冲突策略保持当前实体级“谁更新新谁赢”，不做顺序专用合并

### 5. UI 与交互结论
- `ConnectionListSortField` 新增 `Manual`
- 手动排序选中后：
  - 顶部排序字段按钮显示“手动排序”
  - 升降序按钮禁用或隐藏
  - 工作区 header 可拖拽
  - 连接列表项 / 卡片可拖拽
- 非手动排序模式下不启用拖拽

### 6. 风险点
- 改动跨越实体模型、migration、repository、同步与首页渲染，属于中等偏大功能。
- 但因为“不支持跨工作区拖拽改归属”，复杂度明显可控。
- 关键风险不在拖拽本身，而在：
  - migration 默认顺序是否稳定
  - 云同步字段是否完整
  - 重排后是否只影响当前组

### 7. 交互增强补充（坐标插入预判）
- **实现8**: `main/src/home_tab.rs` 的工作区 header 与连接列表项拖拽逻辑
  - 模式：当前首页拖拽目标只基于“落到哪个目标项上”，没有插入前后语义。
  - 可复用：继续沿用现有 `DragWorkspace` / `DragConnection` payload 与 `reorder_*` 持久化链路。
  - 需注意：只需要把目标索引计算从“目标项槽位”改成“目标项前/后”，不需要改仓库或同步接口。

- **实现9**: `crates/ui/src/dock/tab_panel.rs:899-919`
  - 模式：GPUI 的 `DragMoveEvent<T>` 已能提供 `drag.bounds` 与 `drag.event.position`，可按坐标决定落点区域。
  - 可复用：首页纵向列表可直接按 `y` 坐标上半区 / 下半区判断 `before/after`。
  - 需注意：卡片模式是 `flex-wrap` 网格，不宜和纵向列表共用完全相同的语义。

- **实现10**: `main/src/home_tab.rs:4440-4450`
  - 模式：当前底层移动逻辑是简单的 `remove + insert(target_index)`。
  - 可复用：新增“相对目标前/后插入”的 helper 即可，现有排序落库链路保持不变。
  - 需注意：卡片模式本轮不改，继续保留现有“落到目标项上再重排”的行为。

### 8. 本轮边界
- 本轮只增强：
  - 工作区纵向列表
  - 连接纵向列表模式
- 本轮不增强：
  - 连接卡片模式的坐标插入语义
  - 容器空白区域直接插到末尾
