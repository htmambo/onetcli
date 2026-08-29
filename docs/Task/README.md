# 任务索引

## 活跃任务 (Active)

（暂无）

## 已完成任务 (Archive)

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

