## 项目上下文摘要（home-sync-feedback）
生成时间：2026-03-24 21:36:51 +0800

### 1. 相似实现分析
- 实现1：`main/src/home_tab.rs:321`
  - 模式：同步前置校验失败时把信息写入 `self.cloud_error`
  - 可复用：现有同步状态字段和 `trigger_sync` 主流程
  - 风险点：`cloud_error` 只写不读，界面上没有显式反馈
- 实现2：`main/src/home_tab.rs:1836`
  - 模式：工具栏中已有同步按钮、冲突按钮、主密钥按钮
  - 可复用：在工具栏左侧追加紧邻同步操作的状态展示
  - 风险点：当前按钮 tooltip 只能在 hover 时看到，无法满足失败后显式告知
- 实现3：`main/src/settings/provider_form_dialog.rs:462`
  - 模式：异步任务完成后通过 `cx.active_window + update_window + window.push_notification` 推送通知
  - 可复用：同步完成/失败后沿用同一通知链路
  - 风险点：`trigger_sync` 的回调当前只有 `Context<Self>`，需要在实体更新里取活动窗口再发通知

### 2. 项目约定
- 命名约定：页面内部状态使用 `snake_case` 字段，局部辅助类型可定义在文件内
- 文件组织：首页同步逻辑集中在 `main/src/home_tab.rs`，文案集中在 `main/locales/main.yml`
- UI 反馈模式：短期结果用通知，持续状态可直接渲染在当前页面

### 3. 可复用组件清单
- `window.push_notification(...)`：全局通知入口
- `Button::new("sync-button")` 所在工具栏：同步相关显式反馈的最佳落点
- `t!("Home.*")`：现有首页文案命名空间

### 4. 测试策略
- 构建验证：`cargo test -p main --no-run`
- 单元测试：为同步结果摘要函数补纯逻辑测试，覆盖“无变更 / 成功 / 带错误”三类结果
- 全量回归：`cargo test -p main`

### 5. 依赖和集成点
- 同步入口：`HomePage::trigger_sync`
- 冲突处理入口：`HomePage::resolve_conflicts_individually`
- 底层结果类型：`one_core::cloud_sync::SyncResult`
- 通知层：`gpui_component::notification::Notification`

### 6. 技术选型理由
- 选择“页面内反馈 + 通知”双通道，而不是只补一个 tooltip
- 优势：失败原因可在主界面持续可见，同步结果也能即时弹出提醒
- 风险：仓库暂无直接验证通知弹出的 UI 自动化测试，因此需要用纯逻辑测试和编译验证兜底

### 7. 关键风险点
- 自动同步场景：失败时如果只写日志，用户不会感知
- 部分成功场景：既有冲突又有错误时，结果摘要必须避免信息丢失
- 文案长度：工具栏内联反馈必须可截断，否则容易挤压布局
