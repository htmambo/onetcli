## 项目上下文摘要（home-cross-workspace-drag）
生成时间：2026-03-27 20:05:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:3890-4106`
  - 模式：工作区 section 由 header + 连接集合组成，header 常驻可见，内容区可能折叠。
  - 可复用：把跨工作区连接拖放目标挂在 workspace header，不需要依赖目标组内卡片可见性。
  - 需注意：header 现有 `DragWorkspace` 拖拽排序逻辑，新增 `DragConnection` 时要避免两种 payload 冲突。

- **实现2**: `main/src/home_tab.rs:4128-4515`
  - 模式：列表模式连接拖拽只支持同组内前后插入，使用 `connection_drop_preview` 表达目标连接和插入边。
  - 可复用：同组重排维持现状，跨组迁移不要复用同一个 preview 语义。
  - 需注意：现有逻辑大量使用 `drag.workspace_id != workspace_id` 直接拦截跨组。

- **实现3**: `main/src/home_tab.rs:4708-4855` 与 `main/src/home_tab.rs:4934-5100`
  - 模式：卡片模式使用 overlay 表达中间插槽预览，尾部额外放一个常驻 slot 承接“拖到最后”。
  - 可复用：中间 overlay 保留，尾部 slot 改成仅在连接拖拽预览存在时渲染。
  - 需注意：当前 slot 是常驻布局，只是透明显示，导致非拖拽时也占位。

- **实现4**: `crates/core/src/storage/repository.rs:665-701`
  - 模式：连接更新时如果 `workspace_id` 变化且 `sort_order = None`，仓库会自动为目标组分配末尾顺序。
  - 可复用：跨工作区拖放第一版直接调用 `ConnectionRepository::update(...)`，不新增仓库事务接口。
  - 需注意：这种做法不会压实源组的 `sort_order`，但当前手动排序比较逻辑可接受空洞。

- **实现5**: `crates/db_view/src/db_tree_view.rs:620-770`
  - 模式：数据库树订阅 `ConnectionUpdated` 时只更新已有节点信息，不会根据 `workspace_id` 变化决定移除或新增。
  - 可复用：补一个纯函数判定“移出/移入/更新/忽略”，降低事件分支回归风险。
  - 需注意：如果不补，首页跨组移动后树视图会残留旧节点。

### 2. 本轮边界
- 支持：连接从一个非空、当前可见工作区拖到另一个当前可见工作区的 header。
- 支持：目标工作区折叠时，仍可通过 header 接收拖放。
- 支持：搜索开启时，只要目标工作区 section 仍可见，就允许跨工作区拖放。
- 不支持：拖到空工作区。
- 不支持：通过卡片/列表项在跨组时做精确前后插入。
- 不支持：未分配区作为跨组目标（本轮最小改动先不处理）。

### 3. 设计结论
- 新增 header 级别的跨组连接拖放目标状态，避免和 `connection_drop_preview` 混用。
- 同组内拖拽：
  - 保持现有 `connection_drop_preview + reorder_connections_manually_at(...)`
- 跨组拖拽：
  - 只认工作区 header
  - 落下后把连接移动到目标工作区末尾
  - 复用 `ConnectionRepository::update(...)` 的“换组自动分配末尾顺序”语义

### 4. 风险点
- `db_tree_view` 需要显式处理连接移出当前工作区，否则会有残留节点。
- 搜索态下只对“当前可见工作区”开放落点，属于有意收敛，不是缺陷。
- 源组 `sort_order` 可能产生空洞；当前排序稳定性不受影响，但不是最终最优状态。

### 5. 本地验证预案
- `cargo test -p main home_tab::connection_list_sort_tests -- --nocapture`
- `cargo test -p db_view db_tree_view -- --nocapture`
- `cargo check -p main -p db_view`
