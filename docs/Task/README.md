# 任务索引

## 活跃任务 (Active)

（暂无）

## 已完成任务 (Archive)

### 2026-09
- ✅ [数据库 import/export session 生命周期泄漏修复](Archive/2026-09/SESSION_LEAK_FIX_PLAN.md) - 完成于 2026-09-13（SessionGuard RAII + Drop best-effort close；7 阶段全部完成：P0-1 panic/cancel 兜底 / P0-2 决策单源 / P0-3 事务回滚 / P1-4 共享分类函数 / P1-5 幂等保护 / P1-4 裸 API 私有化 / P2-4 5 类生命周期测试 + 多线程并发 + rollback 追踪；3 个 commit `51ba141f` + `fc465b69` + `556bfd1c`；外部评审 Round 6 + Round 11 verdict=APPROVED；454 db crate 测试通过；0 新增 clippy 警告；session 生命周期问题 P0-1/P0-2 全修复 + 脏复用防护 + cancel/panic 兜底 + 事务回滚）
- ✅ [数据库 session 生命周期 follow-up 改进](Archive/2026-09/SESSION_LEAK_FIX_FOLLOWUP_PLAN.md) - 完成于 2026-09-13（6 轮外部评审迭代 Round 15-20 + Round 21-29 follow-up：perform_release 提取 → ReleaseIntent 合并 → 接线测试矩阵 → 字面量规格 → RfR 永久塌缩文档化 → Drop 入口统一 + 等价特征测试 + DbError::code wire 错误码 + 取消安全修复 + release_fn seam 端到端 Err 测试 + verify_failure_count metric + Drop panic guard；13 个 commit `5d6eabe7` → `a2a3e0ae` → `dd2e35fb` → `b758ad29` → `257a7e87` → `869e1479` → `d5130f75` → `c9748c37` → `7158209b` → `cf286dba` → `436a969b` → `165b9ac7` → `b753758a`；45/45 manager 测试通过 + 三层守卫（编译穷尽 + 运行规格 + 显式不变量）+ DbError 机器可读错误码 + 取消安全（Round 17 P3-5 backlog 关闭）+ cfg(test) mock seam 端到端 Err 测试 + Drop panic guard 防 abort 升级；方案 B（override 视为最新意图）钉死 + RfR 永久塌缩为 Close 设计依据文档化 + Drop ≡ Finish 等价性测试钉死 + Drop 双重 panic 安全不变式文档化）

### 2026-09
- ✅ [dump_sql 上下文菜单扁平化为 3 个独立 Item](Archive/2026-09/SUBMENU_FLATTEN_PLAN.md) - 完成于 2026-09-13，**同日被 `194d5f3d` 撤销**（Submenu hover 根因修复后恢复合并 Submenu；扁平化仅作为过渡妥协方案存在过）
- ✅ [数据库 import/export tokio reactor panic 修复](Archive/2026-09/DB_IMPORT_EXPORT_TOKIO_REACTOR_PANIC_PLAN.md) - 完成于 2026-09-13（修复 MySQL/SQLite 表右键"导出 SQL / 导出表 / 导入表"在 GPUI BackgroundExecutor 上 `tokio::spawn_blocking` 找不到 reactor 的 panic：export_data_with_progress_sync / import_data_with_progress_sync 加 cx: &mut AsyncApp 参数、内部用 Tokio::spawn_result 包裹；3 个调用方从 cx.background_spawn 改为 cx.spawn(async move |cx| { ... })；用户实测 MySQL 转储结构和数据可正常完成导出）
- ✅ [Submenu hover 展开 PopupMenu 框架层修复](Archive/2026-09/SUBMENU_HOVER_POSITION_PLAN.md) - 完成于 2026-09-13（实际根因并非架构限制：父 PopupMenu 的 `popover_style()` 内置 `overflow_hidden` 把 anchored 子菜单裁到父 bounds 内；commit `194d5f3d` 将 popover_style 替换为手写 bg/border/shadow/rounded + 偏移改走 `anchored().offset()`，并**撤销了 4b345b5f 的 dump_sql 扁平化**恢复 Submenu；用户实测 MySQL 转储子菜单 hover 展开 + 子项点击正常。原"方案 A/B/C"架构改造证明不必要。已知残留限制：父菜单 scrollable（>20 项自动开启）时 `overflow_y_scroll` 的 content mask 仍会裁掉子菜单，见 `popup_menu.rs` TODO）
- ✅ [AI 输入框上下箭头历史记录](Archive/2026-09/INPUT_HISTORY_PLAN.md) - 完成于 2026-09-05（提交 `09e8f125` + `d45361fe`：InputHistory 公共状态机 + 三宿主接入；36/36 单测通过；SSH 终端启动 panic 在修复 `cx.on_action` 误用后解决）
- ✅ [AI 输入框历史记录方向 + 浏览语义重构](Archive/2026-09/INPUT_HISTORY_DIRECTION_REWORK_PLAN.md) - 完成于 2026-09-05（按用户需求 ↑/↓ 方向反转；新增 apply_pending_edit 临时副本落定 + can_submit_in_browse 空提交拦截 + escape ESC 复位 + 异步全局历史加载（AiChatPanel 路径）；外部评审 Round 2/4 反馈的 P1-1 ESC 三重守卫已修复；36/36 单测通过；用户实测 SSH terminal AI 助手跨会话历史可见）
- ✅ [AI 助手 Thinking 可折叠面板](Archive/2026-09/THINKING_PANEL_PLAN.md) - 完成于 2026-09-05（新增 crates/core/src/ai_chat/thinking.rs：split_thinking_blocks 解析函数 + ThinkingPanel 可折叠组件；折叠态 60px + flex column + justify_end 让最新思考贴底；集成到 render_assistant_content；markdown.rs 把 cfg!(debug_assertions) warn 降级为 trace；10/10 thinking 单测通过；External Review MCP Round 5 verdict=APPROVED；解决日志中持续刷屏的 `unsupported inline html tag` 警告）

### 2026-08
- ✅ [script/install-linux.sh 适配 Deepin 25 + 补齐 X11 dev 库](Archive/2026-08/INSTALL_LINUX_SH_DEEPIN_DEV_FIX_PLAN.md) - 完成于 2026-08-29（单文件 3 处改动：apt 分支扩为 `ubuntu|debian|linuxmint|pop|deepin|uos|pureos|kali|tails|raspbian|parrot|zorin|elementary|mx|neon`；apt 列表新增 `libxcb1-dev libxkbcommon-dev`；openSUSE 列表新增 `libxkbcommon-devel`；外部评审 Round 2/5 verdict=APPROVED；用户授权 commit 后归档）
- ✅ [AI 终端操作员：多 AI 并发隔离、工具上下文持久化与当前终端语义修复](Archive/2026-08/AI_TERMINAL_OPERATOR_MULTI_AI_FIX.md) - 完成于 2026-08-25（3 commits：`dd6d0ea7` 修 tokio reactor panic + 持久化工具调用中间态；`f91afb66` db_view fmt；`a908ca6d` prompt/schema 改以 host_terminal_id 为当前终端缺省；用户实测三终端并发 + 各自侧栏 AI 助手回复/运行正常）
- ✅ [AI 终端操作员假性终结修复 + read_terminal_output since_last_write](Archive/2026-08/PREMATURE_TERMINATION_DIAGNOSIS.md) - 完成于 2026-08-21（commit message 草稿：[Archive/2026-08/COMMIT_MESSAGE_REMINDER.md](COMMIT_MESSAGE_REMINDER.md)；用户截图复现"继续" 中断；方案 B+D 落地：13 行完成判定矩阵（Length 追加截断提示 / ContentFilter fail-fast / Null 归一化 / Unknown 兜底 + 6 结构化埋点）；effective_max_reminders 钳制；`since_last_write: bool` 参数默认 true（按 terminal_id 跟踪 last_write_lines）；39 单元测试通过；fmt/clippy clean）
- ✅ [终端"自定义高亮"性能优化](Archive/2026-08/TERMINAL_CUSTOM_HIGHLIGHT_PERF_PLAN.md) - 完成于 2026-08-12（4 commits：`b145afce refactor` 抽离 decoration 数据通路与 damage 预解析；`18d54dc4 perf` 解除全量重建钳制，启用脏行增量渲染；`555cb89a perf` char_offset_map 替代 chars().count()；`f49bc012 test` doctest + 单测）

### 2026-07
- ✅ [SSH GEX 最小组尺寸兼容修复](Archive/2026-07/SSH_GEX_MIN_GROUP_SIZE_PLAN.md) - 完成于 2026-07-01（`dca6b674`；RusshClient::connect 统一走 build_client_config；russh 0.60.3 默认 3072 → 2048，修复 4 次 "DH prime size (2048 bits) not within requested range" 后 KexInit 失败）

### 2026-06
- ✅ [终端/SSH ⌘+Click / Win+Click 打开链接失效修复（按平台切换修饰键）](Archive/2026-06/TERMINAL_OPEN_LINK_MODIFIER_PLAN.md) - 完成于 2026-06-15（macOS 仍用 ⌘；Linux/Windows 改为 Ctrl + click；Manjaro 实测通过）
- ✅ [P1 拆分 setting_tab.rs (4455 行 → 2309 行，−49.3%)](Archive/2026-06/SETTING_TAB_SPLIT_PLAN.md) - 完成于 2026-06-15（14 子轮，新增 13 个子模块 2466 行）
- ✅ [旧版激活热键 ctrl+space 自动迁移与 toast 提示](Archive/2026-06/HOTKEY_LEGACY_MIGRATION_PLAN.md) - 完成于 2026-06-10
- ✅ [P2 经验沉淀硬化：RoundedPopup 助手 + bundle-macos codesign](Archive/2026-06/POPUP_CODESIGN_HARDENING_PLAN.md) - 完成于 2026-06-08 (fullauto: popup-codesign)
- ✅ [LLM 提供商云同步问题域归并与回归测试强化](Archive/2026-06/LLM_PROVIDER_SYNC_HARDENING_PLAN.md) - 完成于 2026-06-08
- ✅ [macOS 应用图标内边距优化与白边修复](Archive/2026-06/MACOS_APP_ICON_PADDING_PLAN.md) - 完成于 2026-06-02

### 2026-05
- ✅ [全局性能优化](Archive/2026-05/PERFORMANCE_OPTIMIZATION_PLAN.md) - 完成于 2026-05-21

### 2026-04
- ✅ [UI 主题系统重构](Archive/2026-04/THEME_SYSTEM_REFACTOR_PLAN.md) - 完成于 2026-04-21

---

## 任务创建规范

新任务文档创建在 `docs/Task/Active/`，文件名格式：`TASK_NAME_PLAN.md`

完成后归档到 `docs/Task/Archive/YYYY-MM/` 并更新本索引。

