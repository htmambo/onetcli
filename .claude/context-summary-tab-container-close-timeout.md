## 项目上下文摘要（tab-container-close-timeout）
生成时间：2026-03-30 10:01:15 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/tab_container.rs:1203-1457`
  - 模式：批量关闭标签时直接等待 `Task<bool>` 返回，再决定是否移除标签。
  - 可复用：`do_remove_tab_by_id`、`closing_tabs` 状态清理逻辑。
  - 需注意：这里运行在 GPUI 异步上下文，不能默认具备 Tokio reactor。

- **实现2**: `crates/ui/src/hover_card.rs:167-185`
  - 模式：在 `cx.spawn(...)` 内使用 `cx.background_executor().timer(...)` 做延迟控制。
  - 可复用：GPUI 原生计时器适合替代 `tokio::time::*`。
  - 需注意：计时器应和当前 GPUI task 同一执行模型，避免跨 runtime 依赖。

- **实现3**: `crates/core/src/gpui_tokio.rs:56-95`
  - 模式：需要 Tokio reactor 的 future 必须通过 `Tokio::spawn` / `Tokio::spawn_result` 包装后执行。
  - 可复用：Tokio runtime 接入层已经存在，不需要在普通 `cx.spawn(...)` 中直接调用 Tokio 时间 API。
  - 需注意：仅在确实需要 Tokio 线程池或 reactor 时才进入这层。

### 2. 项目约定
- **命名约定**: 异步任务变量通常使用 `*_task` 命名；状态字段使用语义化集合名如 `closing_tabs`。
- **文件组织**: tab 容器行为集中在 `crates/core/src/tab_container.rs`，运行时桥接在 `crates/core/src/gpui_tokio.rs`。
- **代码风格**: 优先小范围修复，保持现有关闭流程和状态更新顺序不变。

### 3. 可复用组件清单
- `crates/core/src/tab_container.rs::do_remove_tab_by_id`：统一移除标签并清理关闭状态。
- `crates/core/src/gpui_tokio.rs::Tokio::spawn`：Tokio runtime 桥接。
- `gpui::BackgroundExecutor::timer`：GPUI 原生延时能力。

### 4. 测试策略
- **验证方式**: 以本地编译检查为主，确认 `main` 全链路可通过。
- **参考模式**: 该模块当前未找到现成的 `tab_container` 单元测试。
- **覆盖说明**: 本次重点验证编译正确性，并建议人工回归“关闭单个标签页”与“触发慢关闭逻辑”场景。

### 5. 依赖和集成点
- **外部依赖**: `futures::future::select` 用于等待任务与超时二选一。
- **内部依赖**: `TabContent::try_close` 返回 `Task<bool>`，`closing_tabs` 负责防重入。
- **集成方式**: 在 `close_tab` 内保留原有 `entity.update(...)` 状态回写流程。

### 6. 技术选型理由
- **为什么用这个方案**: 目标是最小修复 reactor panic，同时保留 30 秒超时保护。
- **优势**: 不改 `try_close` 接口，不引入新的线程边界，直接复用 GPUI 原生计时器。
- **风险**: 若某些 `try_close` 内部本身仍错误调用 Tokio reactor API，仍可能在其实现内部触发别的 panic。

### 7. 关键风险点
- **边界条件**: 超时分支必须移除 `closing_tabs`，否则标签会永久处于“关闭中”。
- **并发问题**: `close_tab` 有防重复关闭集合，但批量关闭路径没有超时保护，后续若继续报卡死需单独评估。
- **验证缺口**: 当前没有针对慢 `try_close` 的自动化回归测试。

### 8. 工具说明
- 当前会话未提供 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`。
- 本次改用本地代码检索与既有实现比对完成上下文分析，并在操作日志中留痕。
