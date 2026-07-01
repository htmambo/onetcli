# 任务索引

## 活跃任务 (Active)

（暂无）

## 已完成任务 (Archive)

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

