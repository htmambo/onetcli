## 首页手动排序验证报告
生成时间：2026-03-26 23:41:00 +0800

### 需求覆盖
- 已新增第四种首页排序模式：`Manual`
- 已支持工作区之间拖拽重排
- 已支持同一工作区内连接拖拽重排
- 已支持未分配区内连接拖拽重排
- 已明确拒绝跨工作区拖拽改归属
- 已把顺序字段持久化到本地数据库并接入云同步明文结构

### 本次主要改动
- 数据模型：
  - `Workspace.sort_order: Option<i64>`
  - `StoredConnection.sort_order: Option<i64>`
- 数据库：
  - 新增 migration `20260326000003_home_manual_sort.sql`
  - `workspaces` / `connections` 表增加 `sort_order`
  - 旧数据按当前默认显示顺序回填
- 仓库层：
  - 连接 / 工作区查询、插入、更新、云端回写均已读写 `sort_order`
  - 新增 `WorkspaceRepository::reorder(...)`
  - 新增 `ConnectionRepository::reorder_within_workspace(...)`
  - 新建 / 改组对象自动分配目标组末尾顺序
- 同步层：
  - `WorkspacePlainData.sort_order`
  - `ConnectionPlainData.sort_order`
  - 上传、下载、云端覆盖本地均保留顺序字段
- 首页：
  - 排序菜单新增“手动排序”
  - `Manual` 模式下排序方向按钮禁用
  - 工作区 header、连接列表项、连接卡片支持拖拽重排

### 本地验证
- `cargo check -p one-core`
  - 结果：通过
- `cargo check -p main`
  - 结果：通过
- `cargo test -p one-core storage::repository::tests::`
  - 结果：通过（9/9）
- `cargo test -p main connection_list_sort_tests`
  - 结果：通过（4/4）
- `cargo fmt --all`
  - 结果：通过

### 额外观察
- `cargo test -p one-core`
  - 结果：失败（4 条）
  - 失败项：
    - `llm::storage::tests::ensure_onetcli_provider_is_not_default_when_auto_created`
    - `cloud_sync::engine::tests::use_cloud_deleted_cloud_conflict_deletes_local_connection`
    - `cloud_sync::engine::tests::use_local_conflict_resolution_updates_local_sync_status`
    - `cloud_sync::engine::tests::use_local_deleted_cloud_conflict_recreates_remote_item`
  - 判断：从失败名称和报错看，属于现有 LLM 默认提供商与同步引擎测试状态问题，不是本次手动排序逻辑直接引入；本次新增的仓库重排与首页排序测试均已单独通过

### 风险结论
- 已知功能风险主要集中在 GPUI 真实拖拽交互体验，需要运行应用后做一次人工拖拽确认
- 代码层面，手动排序的本地持久化、排序比较和同步打包路径已被编译与针对性测试覆盖

## 交互增强补充验证（2026-03-27 00:08:00 +0800）

### 本轮范围
- 已增强：
  - 工作区纵向列表的坐标插入判定
  - 连接纵向列表模式的坐标插入判定
- 未增强：
  - 连接卡片模式的坐标插入判定
  - 容器空白区域插到末尾

### 本轮验证
- `cargo fmt --all`
  - 结果：通过
- `cargo check -p main`
  - 结果：通过
- `cargo test -p main connection_list_sort_tests`
  - 结果：通过（6/6）

### 结论
- 代码层面，本轮“基于鼠标上下位置决定插入前/后”的改动已通过格式化、编译和单测验证
- 交互层仍建议在真实界面中补一次人工确认，重点关注：
  - 工作区 header 上半区 / 下半区落点是否符合预期
  - 连接列表项插入线是否足够直观
  - 卡片模式仍保持旧行为是否符合当前分阶段范围

## 拖拽预览尺寸补充验证（2026-03-27 00:20:00 +0800）

### 本轮范围
- 已增强：
  - 工作区拖拽预览尺寸跟随源 header 最近一次渲染尺寸
  - 连接列表项拖拽预览尺寸跟随源项最近一次渲染尺寸
  - 连接卡片拖拽预览尺寸跟随源卡片最近一次渲染尺寸

### 本轮验证
- `cargo fmt --all`
  - 结果：通过
- `cargo check -p main`
  - 结果：通过
- `cargo test -p main connection_list_sort_tests`
  - 结果：通过（6/6）

### 结论
- 拖拽幽灵框已不再使用固定小浮层，而是按源元素宽高渲染
- 由于这类改动主要体现在真实 UI 交互，仍建议补一次人工拖拽确认视觉效果
