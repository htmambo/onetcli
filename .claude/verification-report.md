# 验证报告

## 审查补充（window-drag-followup 第三轮）
生成时间：2026-03-29 00:08:00 +0800

### 技术判断
- 第二轮方案虽然已经把 Windows 拖窗从整块 `#tabs` 中拆出，但仍然依赖局部 spacer，实际体验仍可能表现为“只有极窄位置可拖”。
- 第三轮改为让 [`tab_container.rs`](D:\zhp\src\onetcli\crates\core\src\tab_container.rs) 的顶层 `tab-bar` 在 Windows 下直接承担拖窗层，再用 tab、下拉按钮和窗口按钮的 `occlude()` 明确压住交互区域。
- 这更接近通用 [`title_bar.rs`](D:\zhp\src\onetcli\crates\ui\src\title_bar.rs) 的稳定命中模型，也更符合用户对“标签栏空白处都能拖”的预期。

### 验证结果
- `C:\Users\hoping\.cargo\bin\cargo.exe test -p one-core tab_container::tests --lib -- --nocapture`
  - 结果：通过
- `C:\Users\hoping\.cargo\bin\cargo.exe check -p one-core`
  - 结果：通过

### 结论
- 当前最新代码较上一轮更强：不是只给局部 spacer 提供拖窗能力，而是把整个标签栏空白区恢复成稳定拖窗层。
- 剩余验证缺口仍然只有 Windows GUI 实机体感，而不是 Rust 层逻辑闭环。

## 审查报告（window-drag-followup 核对）
生成时间：2026-03-28 23:59:00 +0800

### 需求完整性检查
- 目标明确：复核“窗口不能使用鼠标拖动”是否已在当前工作区闭环
- 范围明确：仅核对主窗口 `TabContainer` 拖窗热区与主窗口 bounds 保存链路，不扩散到无关 UI 模块
- 交付物明确：上下文摘要、操作日志、本地验证结论
- 风险与依赖明确：`main` crate 仍受本机 `cmake` / `nasm` 缺失影响，无法完成全量编译

### 技术维度评分
- 代码质量：92/100
  - 当前工作区中的修复点与仓库既有标题栏设计一致，没有另起平台旁路逻辑
  - Windows 命中区与窗口状态保存性能问题都收敛到了既有模块
- 测试覆盖：83/100
  - `one-core` 中与拖窗平台分支直接相关的 2 条单测已通过
  - `main` 全量编译受本机构建依赖阻塞，GUI 实机验证仍未完成
- 规范遵循：94/100
  - 本次没有覆盖用户已有未提交改动，只补充核对与验证留痕

### 战略维度评分
- 需求匹配：94/100
  - 已确认当前工作区对“不能拖动”的两个真实根因都已有对应修复
- 架构一致：95/100
  - 继续复用 `TabContainer`、`AppSettings`、`Debouncer`，没有引入新的拖窗框架
- 风险评估：86/100
  - 最大剩余风险不是逻辑判断，而是缺少 Windows GUI 实机验证与 `main` 完整构建环境

### 综合评分
- 90/100
- 建议：通过

### 结论
- 当前工作区中，与窗口拖动问题直接相关的修复已经存在且方向正确：
  - [`crates/core/src/tab_container.rs`](D:\zhp\src\onetcli\crates\core\src\tab_container.rs) 已将 Windows 拖窗能力收敛为独立热区，避免 `#tabs` 抢占 tab 拖拽事件
  - [`main/src/onetcli_app.rs`](D:\zhp\src\onetcli\main\src\onetcli_app.rs) 与 [`main/src/setting_tab.rs`](D:\zhp\src\onetcli\main\src\setting_tab.rs) 已把窗口状态保存改为本地缓存 + 防抖写回，避免拖动时高频全局通知和写盘
- 已完成本地验证：
  - `C:\Users\hoping\.cargo\bin\cargo.exe test -p one-core tab_container::tests --lib -- --nocapture`
- 未完成但已明确阻塞原因：
  - `C:\Users\hoping\.cargo\bin\cargo.exe check -p main`
  - 阻塞于 `aws-lc-sys` 依赖构建，当前环境缺少 `cmake` 与 `nasm`

## 审查报告（sftp-context-menu-stability 实现）
生成时间：2026-03-28 04:48:48 +0800

### 需求完整性检查
- 目标明确：修复 SFTP 文件列表右键菜单首次显示错误、条目丢失和按场景删项导致的不稳定问题
- 范围明确：聚焦 `crates/sftp_view/src/file_list_panel.rs` 的菜单构造，复用现有 `context_menu_handler.rs` 动作分发
- 交付物明确：代码修复、本地编译验证、单测验证、操作留痕与审查报告
- 风险与依赖明确：GUI 层的最终弹出效果仍需桌面实测确认

### 技术维度评分
- 代码质量：94/100
  - 修复集中在 `file_list_panel.rs`，没有继续扩散到通用 `context_menu` 底层。
  - 通过“稳定菜单结构 + `.disabled(...)`”表达可用态，避免了继续按条件删项带来的结构漂移。
- 测试覆盖：89/100
  - `cargo check -p sftp_view`、`cargo test -p sftp_view --lib`、`cargo check -p terminal_view` 均已通过。
  - 现有单测覆盖了右键选区同步，但没有自动化覆盖 GUI 弹出菜单的实际视觉内容。
- 规范遵循：95/100
  - 沿用既有 `FileListPanelEvent`、菜单 builder 模式和本地化 key，没有新增临时事件或旁路逻辑。

### 战略维度评分
- 需求匹配：95/100
  - 已将用户明确要求恢复的目录级菜单项补回文件项菜单，并把跨侧动作改为禁用态而非删除。
- 架构一致：93/100
  - 上传/下载仍走 `SftpView` 既有业务链路，本次只修 UI 菜单表达层，没有改动作执行层。
- 风险评估：88/100
  - 主要剩余风险是 `gpui` 运行时的上下文菜单命中细节只能靠界面点测确认，但编译面和逻辑面已闭合。

### 综合评分
- 93/100
- 建议：通过

### 结论
- 根因已确认并修复：
  - `crates/sftp_view/src/file_list_panel.rs` 的 `build_panel_context_menu(...)` 存在破坏性的 builder 链错误，直接导致当前代码不可稳定维护。
  - 文件项菜单此前通过 `is_remote` / `is_dir` 直接删项，导致菜单结构不稳定，容易出现“第一次缺项、后续条目变化”的体验问题。
- 修复方式：
  - [`crates/sftp_view/src/file_list_panel.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/file_list_panel.rs) 中重写文件项与空白区菜单结构，改为稳定菜单 + 禁用态控制。
  - 继续复用 [`crates/sftp_view/src/context_menu_handler.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/context_menu_handler.rs) 中现有 `UploadSelected` / `Download` 分发，不重写业务逻辑。
- 本地验证通过：
  - `cargo check -p sftp_view`
  - `cargo test -p sftp_view --lib`
  - `cargo check -p terminal_view`
- 下一步应以 GUI 点测为准，重点确认第一次右键即出现正确菜单，且菜单点击后不再发生条目突变。

---

- 时间：2026-03-24
- 任务：修复 `crates/core/src/llm/connector.rs` 在升级 `llm-connector` 后的编译失败
- 审查结论：通过
- 综合评分：91/100

## 技术维度评分
- 代码质量：94/100
  - 改动集中在 `connector.rs`，保持了既有 `ProviderType` 分支结构。
  - 旧 API 调用已全部替换为 `llm-connector 1.1.14` 的显式 `base_url` 形式。
- 测试覆盖：82/100
  - 新增了 `provider_base_url` 的两个单元测试。
  - 受 `gpui` Metal shader 构建脚本和沙箱限制影响，`cargo test` 未能完成全流程执行。
- 规范遵循：96/100
  - 仅改动必要文件，命名、导入顺序、错误处理风格与项目现状一致。

## 战略维度评分
- 需求匹配：95/100
  - 直接修复了升级后的编译错误，并保留 `api_base` 可选时的既有体验。
- 架构一致：93/100
  - 未修改 `LlmProvider`、`ProviderManager`、`ProviderConfig` 接口，模块边界稳定。
- 风险评估：86/100
  - 默认 URL 来源已用本地 crate 源码和 README 示例校验。
  - 剩余风险主要来自运行环境对 `cargo test` 的限制，而非实现本身。

## 验证结果
- 已执行：`cargo check -p one-core`
  - 结果：通过
- 已执行：`cargo test -p one-core provider_base_url --lib`
  - 结果：失败
  - 原因：`gpui` 构建脚本写 `~/.cache/clang/ModuleCache` 被沙箱拒绝，报错为 Metal shader compilation failed
- 已执行：`CLANG_MODULE_CACHE_PATH=/tmp/clang-cache cargo test -p one-core provider_base_url --lib`
  - 结果：失败
  - 原因：同上，构建脚本未遵循该环境变量

## 审查清单
- 需求字段完整性：已确认目标、范围、交付物、审查要点
- 原始意图覆盖：无遗漏，聚焦编译失败修复
- 交付物映射：代码、上下文摘要、操作日志、验证报告均已生成
- 依赖与风险评估：已完成
- 审查留痕：已完成

## 建议
- 当前改动可以合并。
- 若需要补全自动化验证，建议在允许写用户缓存目录的环境下重跑 `cargo test -p one-core provider_base_url --lib`，或为 `gpui` 构建脚本单独配置可写模块缓存路径。

---

## 审查报告（deepin-window-control-corner 实现）
生成时间：2026-03-28 02:00:23 +0800

### 需求完整性检查
- 目标明确：重新定位 Deepin 下主窗口右上角关闭按钮越出圆角的问题，并给出可落地修复
- 范围明确：主窗口标签栏、通用标题栏、窗口边框与 X11 平台证据链
- 交付物明确：代码实现、上下文摘要、操作日志、本地验证记录
- 风险与依赖明确：Deepin 实际视觉结果仍需桌面实测

### 技术维度评分
- 代码质量：93/100
  - 改动集中在两个按钮容器，不触碰平台层与窗口创建逻辑。
  - 修复直接绑定已有 Deepin 环境判断与 `window.window_decorations()`，约束清晰。
- 测试覆盖：84/100
  - 复用了现有 Deepin 识别单测，并完成相关 crate 编译验证。
  - 受环境限制，无法自动截图确认最终视觉效果。
- 规范遵循：95/100
  - 仅增加最小布局逻辑，并补齐了上下文与操作留痕。

### 战略维度评分
- 需求匹配：94/100
  - 先拿到真实窗口属性和窗口树证据，再实施修复，避免继续在错误层级补丁。
- 架构一致：92/100
  - 保持平台层不变，把修复收敛为 Deepin 兼容布局调整，符合当前证据。
- 风险评估：87/100
  - 已明确剩余风险为圆角半径主题差异，而不是 X11 属性缺失。

### 综合评分
- 91/100
- 建议：通过

### 结论
- 现场证据已经排除“属性没写对”的假设：主窗口 `0x8000002` 的 `_DEEPIN_NO_TITLEBAR=1`、`_DEEPIN_FORCE_DECORATE=0` 都正确。
- 现场证据确认 Deepin 仍然给主窗口包裹了额外无名父窗口，且应用窗口和父窗口都没有 X11 shape，因此右上角问题更接近壳层圆角与内容布局不共用裁剪面。
- 修复策略改为“关闭按钮独立圆角裁剪”而不是继续追加无效裁剪：
  - [`crates/core/src/tab_container.rs`](/usr/htdocs/onetcli/crates/core/src/tab_container.rs) 为主窗口关闭按钮增加 Deepin 专用圆角包装层
  - [`crates/ui/src/title_bar.rs`](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs) 为通用标题栏关闭按钮同步增加同样逻辑
- 本地验证通过：`cargo test -p gpui-component title_bar::tests -- --nocapture`、`cargo check -p gpui-component -p one-core -p main`
- 剩余工作仅为 Deepin 实机视觉确认；若仍有轻微越界，应优先微调关闭按钮包装层的圆角半径，而不是回退到平台属性层

---

## 审查报告（auto-switch-theme 实现）
生成时间：2026-03-28 02:35:25 +0800

### 需求完整性检查
- 目标明确：修复设置页“自动切换主题”无论勾选与否都看不到效果的问题
- 范围明确：设置持久化、主题生效逻辑、主窗口系统外观变化监听
- 交付物明确：代码实现、上下文摘要、操作日志、针对性单测、本地编译验证
- 风险与依赖明确：GUI 级实测仍需桌面环境验证

### 技术维度评分
- 代码质量：94/100
  - 根因修复集中在 `AppSettings` 和主窗口初始化，不扩散到无关 UI 组件。
  - 纯判定函数与副作用逻辑已分离，便于后续维护和测试。
- 测试覆盖：89/100
  - 新增了“自动切换关闭/开启”两条单元测试。
  - 受当前终端环境限制，无法自动化验证桌面主题切换后的实际界面观感。
- 规范遵循：96/100
  - 沿用现有 `Theme::change(...)` 和窗口观察接口，没有引入新的主题状态系统。

### 战略维度评分
- 需求匹配：96/100
  - 同时修复了“勾选/取消无即时效果”和“系统亮暗变化不跟随”两类问题。
- 架构一致：94/100
  - 设置仍由 `AppSettings` 主导，窗口事件仍由主窗口生命周期接入，边界清晰。
- 风险评估：88/100
  - 剩余风险主要是不同平台对 `WindowAppearance` 事件触发时机的差异，但主路径已完整闭环。

### 综合评分
- 93/100
- 建议：通过

### 结论
- 根因已确认：
  - [`main/src/setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs) 里 `auto_switch_theme` 原先只保存，不参与主题模式选择
  - 设置项变更后没有立即重新应用主题
  - [`main/src/onetcli_app.rs`](/usr/htdocs/onetcli/main/src/onetcli_app.rs) 原先没有注册窗口外观变化监听
  - 当前 Deepin/X11 会话的 `xdg-desktop-portal` 不提供 `org.freedesktop.appearance color-scheme`，默认系统外观来源不可用
- 修复方式：
  - 在 [`main/src/setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs) 中新增“有效主题模式”计算和统一主题应用入口
  - 在 [`main/src/setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs) 中增加 Deepin `gsettings theme-name` 回退
  - 在 [`main/src/onetcli_app.rs`](/usr/htdocs/onetcli/main/src/onetcli_app.rs) 中为主窗口注册 `observe_window_appearance(...)` 和 `observe_window_activation(...)`
- 本地验证通过：
  - `cargo test -p main 自动切换 --bin onetcli -- --nocapture`
  - `cargo check -p main`

---

## 审查报告（home-cross-workspace-drag 实现）
生成时间：2026-03-27 20:38:18 +0800

### 需求完整性检查
- 目标明确：首页连接卡片拖拽支持跨工作区移动，且虚拟占位只在拖拽时出现
- 范围明确：主要落在 `main/src/home_tab.rs`，并补齐 `crates/db_view/src/db_tree_view.rs` 的连接换组订阅行为
- 交付物明确：代码实现、上下文摘要、操作日志、专项验证报告、本地验证结果
- 风险与依赖明确：空工作区/未分配区不支持作为目标；跨区落下仍统一进入目标工作区末尾

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：88/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：93/100
- 风险评估：87/100

### 综合评分
- 93/100
- 建议：通过

### 结论
- 占位逻辑已收敛：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs#L3080) 现在会同时清理 `workspace_drop_preview`、`connection_drop_preview` 和 `connection_workspace_drop_target`，避免非拖拽状态下残留虚拟占位。
- 跨区拖拽已打通：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs#L4125) 为工作区标题补充连接跨区落点，[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs#L4267) 又把工作区内容区外层容器也变成跨区落点，不再局限于标题区域。
- 精准落位已补齐：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs#L3580) 之后新增跨工作区移动计划与提交链路，列表 gap、列表项、卡片项、卡片尾部 slot 在跨区时都能按目标位置插入，而不再只进末尾。
- 同区排序未被破坏：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs#L4569) 之后的列表/卡片排序链路仍保留原有同区重排逻辑，只在 `drop` 分支上增加跨区插入路径。
- 树视图同步已补齐：[`db_tree_view.rs`](/usr/htdocs/onetcli/crates/db_view/src/db_tree_view.rs#L626) 会在连接换工作区时根据跟踪状态执行 `Remove/Update/Add/Ignore`，避免首页移动后树节点残留在旧工作区。
- 仓储排序一致性已补齐：[`repository.rs`](/usr/htdocs/onetcli/crates/core/src/storage/repository.rs#L365) 新增 `move_across_workspaces(...)`，在单个事务里完成换组、源组压实与目标组重排。
- 本地验证有效：`cargo test -p one-core connection_repository_move_across_workspaces --lib -- --nocapture`、`cargo test -p main connection_list_sort_tests -- --nocapture` 与 `cargo check -p main -p db_view -p one-core` 均已通过。

---

## 审查报告（ssh-agent-auth 实现）
生成时间：2026-03-24 09:35:05 +0800

### 需求完整性检查
- 目标明确：为 SSH 连接补齐 `ssh-agent` 认证支持
- 范围明确：覆盖存储模型、SSH/SFTP 认证、终端 SSH 表单、数据库 SSH 隧道与本地化文案
- 交付物明确：代码实现、上下文摘要、操作日志、验证报告、本地验证结果
- 风险与依赖明确：终端/UI 相关验证依赖 `gpui` 构建链，已通过提权本地校验完成

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：90/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 认证能力链路已闭环：[`models.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/storage/models.rs#L244) 新增 `SshAuthMethod::Agent`，[`ssh.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ssh/src/ssh.rs#L55) 新增 `SshAuth::Agent` 并实现 agent 认证流程。
- 重复逻辑已收敛：[`russh_impl.rs`](/Users/hufei/RustroverProjects/onetcli/crates/sftp/src/russh_impl.rs#L47) 不再维护单独的密码/私钥认证逻辑，而是复用 `ssh::authenticate_session`。
- UI 与配置映射已补齐：[`ssh_form_window.rs`](/Users/hufei/RustroverProjects/onetcli/crates/terminal_view/src/ssh_form_window.rs#L164) 新增 `Agent` 单选项，相关 `SshAuthMethod -> SshAuth` 映射点也已在终端、SFTP 视图和文件管理器侧补齐。
- 数据库隧道已支持：[`ssh_tunnel.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/ssh_tunnel.rs#L117) 现在能解析 `ssh_auth_type=agent`，[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L271) 也增加了对应选项。
- 本地验证有效：`cargo check -p ssh -p sftp`、`cargo test -p ssh -p sftp`、提权后的相关 crate `cargo check`、`db` 与 `one-core` 的新增测试都已通过。

---

## 审查报告（ollama-thinking-fallback 实现）
生成时间：2026-03-24 13:33:30 +0800

### 需求完整性检查
- 目标明确：修复 Ollama 下 `qwen3:14b` 等模型正文为空但 `thinking` 有值时，聊天界面显示空回复的问题。
- 范围明确：只修改 `one-core` 项目侧流式消费逻辑，不改第三方 `llm-connector`。
- 交付物明确：共享 helper、双路径修复、最小单测、本地验证、上下文与操作留痕。
- 风险与依赖明确：潜在风险是 reasoning 与正文混合展示；当前修复以可用性优先解决空回复。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：90/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：95/100
- 风险评估：88/100

### 综合评分
- 93/100
- 建议：通过

### 结论
- 修复点集中且边界清晰：[`mod.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/mod.rs#L19) 新增 `extract_stream_text`，正文优先、正文为空时回退 reasoning/thinking。
- 两条消费链路已统一：[`stream.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/ai_chat/stream.rs#L15) 与 [`general_chat.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/agent/builtin/general_chat.rs#L8) 都改为复用共享 helper，不再各自只读 `get_content()`。
- 问题现象已与本机运行时对齐：提权访问本机 Ollama 证明 `qwen3:14b` 的确会返回空 `content` 和非空 `thinking`，本次修复直接覆盖这一场景。
- 本地验证有效：`cargo check -p one-core` 与 `cargo test -p one-core extract_stream_text --lib` 均已通过。

---

## 审查报告（aliyun-qwen35-url 实现）
生成时间：2026-03-25 10:32:13 +0800

### 需求完整性检查
- 目标明确：修复阿里云官方 `qwen3.5-plus` 在 onecli 中因 URL 路径错误导致的 parse error。
- 范围明确：只调整 `one-core` 的 Aliyun client 路由，不修改第三方 `llm-connector`。
- 交付物明确：`connector.rs` 最小补丁、单元测试、本地验证、上下文与日志留痕。
- 风险与依赖明确：仅对 `qwen3.5-*` 或显式 `compatible-mode` 地址切换为 OpenAI 兼容路径，降低对现有普通模型的影响。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：89/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- 根因已修正：[`connector.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/connector.rs#L15) 新增阿里云 compatible-mode 默认地址，并在 [`connector.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/connector.rs#L59) 为 `qwen3.5-*` 与显式 `compatible-mode` 地址改走 `openai_compatible`。
- 兼容边界清晰：[`connector.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/connector.rs#L145) 的 `aliyun_prefers_compatible_mode` 只匹配明确场景，其余阿里云模型仍保留原生 `aliyun/aliyun_private` 路径。
- 本地验证有效：`cargo test -p one-core aliyun_prefers_compatible_mode --lib` 与 `cargo check -p one-core` 均已通过。

---

## 审查报告（aliyun-provider-cache 实现）
生成时间：2026-03-25 10:38:33 +0800

### 需求完整性检查
- 目标明确：修复阿里云 qwen3.5-plus 在运行时仍复用旧 provider 导致 URL 错误继续存在的问题。
- 范围明确：仅增强 `ai_chat` provider 创建配置与 `ProviderManager` 缓存命中条件。
- 交付物明确：缓存签名补丁、模型覆盖补丁、本地验证、上下文与操作留痕。
- 风险与依赖明确：补丁不会修改第三方库，只影响配置变化时的 provider 重建。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：88/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：96/100
- 风险评估：91/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 运行时根因已闭环：[`stream.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/ai_chat/stream.rs#L205) 在创建 provider 前把当前 `selected_model` 写回临时 `provider_config.model`。
- 缓存误复用已修正：[`manager.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/manager.rs#L13) 新增 `ProviderCacheEntry`，[`manager.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/src/llm/manager.rs#L31) 开始按配置签名而非仅按 id 命中缓存。
- 本地验证有效：`cargo test -p one-core provider_cache_signature_changes_with_model --lib` 与 `cargo check -p one-core` 均已通过。

---

## 审查报告（db-tree-csv-import-target 实现）
生成时间：2026-03-24 18:40:00 +0800

### 需求完整性检查
- 目标明确：修复从数据库树表节点导入 CSV/TXT/JSON/SQL 时忽略 database/schema，导致写入默认库同名表的问题。
- 范围明确：限定在 `crates/db/src/import_export/formats/*`，不改 UI 和 manager 接口。
- 交付物明确：共享 helper、导入修复、单元测试、本地验证、上下文与操作留痕。
- 风险与依赖明确：风险主要是行为从“错误写入默认库”修正为“写入选中库”，属于预期缺陷修复。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：90/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：92/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 导入目标表定位已统一：[`mod.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/import_export/formats/mod.rs#L15) 新增 `format_import_table_reference`，直接复用 `DatabasePlugin::format_table_reference`。
- 受影响导入格式已修正：[`csv.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/import_export/formats/csv.rs#L135)、[`json.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/import_export/formats/json.rs#L32)、[`txt.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/import_export/formats/txt.rs#L51)、[`sql.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/import_export/formats/sql.rs#L52) 不再使用裸表名执行 `TRUNCATE/INSERT`。
- 回归验证有效：`cargo test -p db format_import_table_reference --lib` 与 `cargo check -p db` 均已通过。

---

## 审查报告（db-tree-filter-persist 实现）
生成时间：2026-03-24 18:46:00 +0800

### 需求完整性检查
- 目标明确：修复取消勾选数据库后重新进入数据库页仍显示旧筛选结果的问题。
- 范围明确：限定在 `db_tree_view` 的筛选保存和连接事件同步逻辑，不改存储 schema。
- 交付物明确：筛选同步 helper、事件广播修复、纯逻辑测试、本地验证、上下文与操作留痕。
- 风险与依赖明确：主页连接列表依赖连接更新事件，本次修复直接复用该链路。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：89/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：91/100

### 综合评分
- 94/100
- 建议：通过

### 结论

- 根因已修复：[`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L1000) 在筛选写库成功后会发 `ConnectionUpdated`，不再只更新仓库而忽略内存连接列表。
- 打开的树视图也会同步筛选状态：[`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L725) 现在会从传入的 `StoredConnection` 刷新 `selected_databases`。
- 纯逻辑测试已覆盖“保留筛选”和“恢复全选”：[`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L2669)。
- 本地验证有效：`cargo test -p db_view sync_selected_databases_from_connection --lib` 与 `cargo check -p db_view` 均已通过。

---

## 审查报告（db-tree-refresh-tokio-runtime 实现）
生成时间：2026-03-25 10:45:01 +0800

### 需求完整性检查
- 目标明确：修复数据库树刷新时因 `tokio::fs` 在非 Tokio runtime 中执行而触发 panic 的问题。
- 范围明确：限定在 `db_tree_view` 的刷新任务调度，不改缓存模块公开接口。
- 交付物明确：运行时切换修复、本地编译验证、现有测试回归、上下文与操作留痕。
- 风险与依赖明确：依赖项目已有 `Tokio` 包装，风险主要是后台任务调度边界调整。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：86/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：98/100
- 风险评估：92/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 根因已闭环：[`cache.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/cache.rs#L277) 的缓存失效逻辑使用 `tokio::fs`，而 [`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L1128) 之前在 GPUI task 中直接 await，运行时上下文不匹配。
- 修复符合项目既有模式：[`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L1128) 现在通过 `Tokio::spawn` 把缓存和元数据失效切到共享 Tokio runtime，再回 UI 线程重建树。
- 失败可见性更好：[`db_tree_view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/db_tree_view.rs#L1147) 新增 Tokio 任务失败日志，避免异常被静默吞掉。
- 本地验证有效：`cargo check -p db_view` 与 `cargo test -p db_view sync_selected_databases_from_connection --lib` 均已通过。

---

## 审查报告（db-connection-form-ssl 实现）
生成时间：2026-03-25 13:35:05 +0800

### 需求完整性检查
- 目标明确：为 `db_connection_form` 实现可用的 SSL 配置，并让保存的参数真正影响建连逻辑。
- 范围明确：UI 字段、i18n 文案、MySQL/PostgreSQL 驱动建连、ClickHouse TLS feature、MSSQL 分组调整。
- 交付物明确：SSL 标签页字段、驱动层参数接入、依赖特性启用、纯逻辑测试、本地验证、上下文与操作留痕。
- 风险与依赖明确：PostgreSQL 需要外部 TLS connector，MySQL/ClickHouse 需要启用 TLS feature。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：88/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：97/100
- 风险评估：91/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- 表单层已补齐 SSL 配置：[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L333) 开始为 MySQL/PostgreSQL/MSSQL/ClickHouse 提供非空 SSL 分组；Oracle 的空白 SSL 标签页已移除。
- MySQL SSL 已接入：[`mysql/connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/mysql/connection.rs#L31) 新增 `build_ssl_opts`，根据 `require_ssl/verify_ca/verify_identity/ssl_root_cert_path/tls_hostname_override` 构造 `mysql_async::SslOpts`。
- PostgreSQL TLS 已接入：[`postgresql/connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/postgresql/connection.rs#L39) 新增 `ssl_mode` 与 TLS connector 构建逻辑，不再固定 `NoTls`。
- 依赖特性已对齐：[`Cargo.toml`](/Users/hufei/RustroverProjects/onetcli/Cargo.toml#L94) 为 `mysql_async`、`clickhouse` 开启 TLS feature，并新增 `postgres-native-tls` / `native-tls`。
- 本地验证有效：`cargo check -p db_view`、`cargo test -p db ssl_ --lib`、`cargo test -p db_view ssl_tab --lib` 全部通过。

---

## 审查报告（db-ssl-rustls-migration 实现）
生成时间：2026-03-25 14:30:01 +0800

### 需求完整性检查
- 目标明确：将数据库 SSL 实现从 `native-tls` 迁移到 `rustls`，同时保持 `db_connection_form` 的字段和 `extra_params` 契约不变。
- 范围明确：工作区依赖、`db` crate 依赖、PostgreSQL connector、MySQL/ClickHouse/MSSQL 的 TLS feature。
- 交付物明确：依赖迁移、PostgreSQL rustls connector、补充测试、本地验证、上下文与操作留痕。
- 风险与依赖明确：PostgreSQL 是唯一需要替换 connector 的驱动，其余驱动主要依赖 feature 切换。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：91/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 依赖已切换到 rustls：[`Cargo.toml`](/Users/hufei/RustroverProjects/onetcli/Cargo.toml#L97) 现改用 `mysql_async` 的 `rustls-tls`、`clickhouse` 的 `rustls-tls-native-roots`、`tokio-postgres-rustls`，并移除了工作区对 `native-tls/postgres-native-tls` 的直接依赖。
- PostgreSQL connector 已替换：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/postgresql/connection.rs#L209) 通过 `MakeRustlsConnect` 建连，不再依赖 `native_tls::TlsConnector`。
- 现有参数语义被保留：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/postgresql/connection.rs#L31) 的包装 verifier 仅针对 `ssl_accept_invalid_certs` / `ssl_accept_invalid_hostnames` 放宽对应证书错误，不影响其它 TLS 校验路径。
- 自定义 CA 仍可用：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/postgresql/connection.rs#L146) 同时支持从 `ssl_root_cert_path` 读取 PEM/DER 证书并叠加到系统根证书。
- 本地验证有效：`cargo check -p db_view`、`cargo test -p db ssl_ --lib`、`cargo test -p db_view ssl_tab --lib` 全部通过。

---

## 审查补充（mysql-ssh-tls-lab 镜像复用调整）
生成时间：2026-03-25 15:09:17 +0800

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：78/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：97/100
- 风险评估：84/100

### 综合评分
- 89/100
- 建议：需讨论

### 结论
- 需求已落实：[`docker-compose.yml`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/docker-compose.yml#L3) 已默认复用 `mysql:8.4.5`，[`verify.sh`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/verify.sh#L10) 与之保持一致。
- 文档已对齐：[`README.md`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/README.md#L13) 说明了默认镜像与 `MYSQL_IMAGE` 覆盖方式。
- 当前唯一未闭环项不是 MySQL 镜像，而是 bastion 首次构建依赖的 `ubuntu:24.04` 拉取/构建仍在进行，因此整套 SSH+TLS 自动验证尚未最终通过。

---

## 审查补充（本地 Docker MySQL TLS + 远程 sshd 联调准备）
生成时间：2026-03-25 16:27:27 +0800

### 技术维度评分
- 代码质量：92/100
- 测试覆盖：90/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：97/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- 本地 TLS MySQL 已可用：[`docker-compose.yml`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/docker-compose.yml#L2) 的 `mysql` 服务已成功启动，健康检查通过。
- 容器内 TLS 校验通过：使用 `VERIFY_IDENTITY` 和 CA 文件查询 [`01-init.sql`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/mysql/init/01-init.sql#L1) 初始化的 `smoke_test` 表，结果为 `2`。
- 宿主机路径也通过：使用本机 `mysql` 客户端连接 `127.0.0.1:33306` 并携带 [`ca.pem`](/Users/hufei/RustroverProjects/onetcli/.claude/mysql-ssh-tls-lab/mysql/certs/ca.pem#L1) 做 `VERIFY_IDENTITY` 校验成功，说明后续经远程 sshd 反向转发到本地 Docker MySQL 的链路具备基础条件。
- `host.docker.internal` 校验失败不影响本次方案：证书 SAN 针对的是 `127.0.0.1`/`localhost`/`mysql`，而 onetcli 通过 SSH 隧道建立本地转发后实际连接主机同样是 `127.0.0.1`。 

---

## 审查补充（MySQL rustls provider panic 修复）
生成时间：2026-03-25 18:40:56 +0800

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：86/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：97/100
- 风险评估：92/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 根因已闭环：[`mysql/connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/mysql/connection.rs#L37) 之前在启用 TLS 时直接进入 `mysql_async` 的 rustls connector，但进程级默认 `CryptoProvider` 未安装，导致运行时 panic。
- 修复方式与仓库既有模式一致：新增 [`rustls_provider.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/rustls_provider.rs#L1) 统一封装 `aws_lc_rs::default_provider().install_default().ok()`，并用 `Once` 保证只初始化一次。
- 驱动复用已收口：[`mysql/connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/mysql/connection.rs#L37) 和 [`postgresql/connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/postgresql/connection.rs#L210) 现在都复用同一个 helper，避免 TLS 初始化逻辑继续分叉。
- 本地编译验证有效：`cargo check -p db` 已通过。

---

## 审查报告（bracketed-paste-fallback 实现）
生成时间：2026-03-25 16:12:44 +0800

### 需求完整性检查
- 目标明确：修复远端未开启 bracketed paste 时，多行粘贴尤其 heredoc 被 shell 错误续行解析的问题。
- 范围明确：限定在 `TerminalView` 的粘贴入口、相关文案和本地单元测试，不改 `Terminal`/`PTY` 透明传输层。
- 交付物明确：高风险粘贴拦截、上下文摘要、操作日志、本地测试与审查报告。
- 风险与依赖明确：shell 结构识别是启发式；依赖 `alacritty_terminal::TermMode` 提供 `ALT_SCREEN` 与 `BRACKETED_PASTE` 状态。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：90/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：98/100
- 风险评估：92/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 高风险结构已在视图层统一拦截：[`view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/terminal_view/src/view.rs#L113) 新增 `UnbracketedPasteHazard` 和相关纯函数，覆盖 heredoc、未闭合引号与反斜杠续行。
- 粘贴决策已从“只确认”升级为“必要时阻断”：[`view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/terminal_view/src/view.rs#L1231) 在无 `BRACKETED_PASTE` 时先检查高风险块，再决定是否允许进入原有多行确认流程。
- 协议语义保持正确：[`view.rs`](/Users/hufei/RustroverProjects/onetcli/crates/terminal_view/src/view.rs#L1252) 的 `paste_text_unchecked` 仍只在远端程序已开启 `BRACKETED_PASTE` 时发送 `\x1b[200~...\x1b[201~`，没有伪造远端能力。
- 用户提示已补齐：[`main.yml`](/Users/hufei/RustroverProjects/onetcli/main/locales/main.yml#L1169) 新增 `TerminalView` / `TerminalSidebar` 相关文案，避免新对话框显示原始 key。
- 本地验证有效：`env CLANG_MODULE_CACHE_PATH=/tmp/clang-cache cargo test -p terminal_view --lib` 在沙箱外通过，13 个测试全部通过。

### 剩余风险
- 当前 shell 结构检测是启发式规则，不覆盖所有复杂复合语法；但已覆盖用户报告的 heredoc 主故障路径和两类常见续行结构。
- `main/locales/main.yml` 中补入了当前代码已在使用但仓库缺失的 `TerminalView` / `TerminalSidebar` 文案键，若后续有专门的本地化整理任务，可再统一清理同类缺口。

---

## 审查报告（home-encourage-tab 实现）
生成时间：2026-03-25 19:39:55 +0800

### 需求完整性检查
- 目标明确：将首页“支持作者”从弹框改为在 `tab_container` 中打开页签，并改善赞赏码展示空间。
- 范围明确：限定在首页按钮入口、赞赏视图自身和 `HomePage` 页签打开辅助方法。
- 交付物明确：代码修改、上下文摘要、操作日志、验证报告。
- 风险与依赖明确：整仓编译当前被既有 `db_connection_form.rs` 错误阻塞，因此只能做局部无新增错误验证。

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：78/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：97/100
- 风险评估：86/100

### 综合评分
- 90/100
- 建议：通过

### 结论
- 赞赏视图已转为页签内容：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 中的 `EncouragePanel` 现在实现了 `EventEmitter<TabContentEvent>` 和 `TabContent`，可以直接挂入 `TabContainer`。
- 首页入口已切换为单实例页签：[`home_tabs.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/home/home_tabs.rs) 新增 `add_encourage_tab`，复用 `activate_or_add_tab_lazy`，重复点击只会激活已有页签。
- 原弹框路径已移除：[`home_tab.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/home_tab.rs) 的“支持作者”按钮已改为调用 `add_encourage_tab`，不再走 `window.open_dialog`。
- 展示尺寸已放大：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 将二维码尺寸从 `180` 调整到 `220`，更适合主内容区域。

### 剩余风险
- 当前无法给出“整仓编译通过”结论，因为 `crates/db_view/src/common/db_connection_form.rs` 已存在与本次无关的编译错误。
- 如果后续项目启用统一的 `TabContentRegistry` 恢复注册，建议再补 `EncouragePanel` 的恢复逻辑；本次未做这部分扩展。

---

## 审查报告（oracle-connection 实现）
生成时间：2026-03-26 09:12:31 +0800

### 需求完整性检查
- 目标明确：修正 `crates/db/src/oracle/connection.rs` 的 Oracle 查询取值逻辑，使其能稳定处理 `chrono` 日期时间与常见 Oracle 类型。
- 范围明确：改动限定在 Oracle 连接层值提取与列类型显示，不触碰连接配置、插件接口和上层查询结果结构。
- 交付物明确：代码修改、上下文摘要、操作日志、本地编译验证和审查报告均已落地。
- 风险与依赖明确：依赖 `oracle 0.6.3` 的 `chrono` 特性和 `OracleType` 枚举；当前缺少真实 Oracle 集成环境。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：78/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：97/100
- 风险评估：84/100

### 综合评分
- 91/100
- 建议：通过

### 结论
- Oracle 结果提取已改为类型驱动：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/oracle/connection.rs) 现在基于 `OracleType` 分支读取 `Date/Timestamp/TimestampTZ/TimestampLTZ/Raw/BLOB/BFILE/Boolean/Number` 等类型，不再只依赖 `String/i64/f64` 的宽泛尝试。
- `chrono` 类型已真正接入：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/oracle/connection.rs) 新增 `NaiveDateTime` 与 `DateTime<FixedOffset>` 的格式化 helper，日期时间输出风格与 PostgreSQL/MSSQL 当前实现保持一致。
- 二进制结果展示已统一：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/oracle/connection.rs) 对 `RAW/BLOB/BFILE` 使用 `0x...` 文本输出，避免表格层出现不可读字节。
- 列元数据显示更稳定：[`connection.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db/src/oracle/connection.rs) 将 Oracle 列类型元数据从 `Debug` 输出改为 `Display` 字符串，便于前端展示。
- 本地验证有效：`rustfmt --edition 2021 crates/db/src/oracle/connection.rs` 与 `cargo check -p db` 均已通过。

### 剩余风险
- 当前没有真实 Oracle 数据库的本地自动化测试，无法确认所有 Oracle 会话设置和特殊列类型在运行时都能命中预期分支。
- `CLOB/NCLOB/REF CURSOR/Object` 仍保留字符串兜底路径；如果后续出现具体运行时样例，可能需要继续细化映射。

---

## 审查报告（typos-ci-fix 实现）
生成时间：2026-03-26 10:12:34 +0800

### 需求完整性检查
- 目标明确：移除仓库中的 `typos` 检查链路，消除 GitHub 流程中的相关失败。
- 范围明确：包含 CI workflow、根 `Cargo.toml` 工具配置，以及 README / README_CN / CLAUDE 的开发命令说明。
- 交付物明确：代码修改、上下文摘要、操作日志、本地验证和审查报告均已落地。
- 风险与依赖明确：`.claude` 历史记录仍会保留 `typos` 字样，但它们不属于生效检查入口。

### 技术维度评分
- 代码质量：97/100
- 测试覆盖：88/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：96/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- CI 已移除 `typos` 检查：[`ci.yml`](/Users/hufei/RustroverProjects/onetcli/.github/workflows/ci.yml) 删除了 `Typo check` 步骤，GitHub workflow 不再安装或执行 `typos-cli`。
- 根配置已清理：[`Cargo.toml`](/Users/hufei/RustroverProjects/onetcli/Cargo.toml) 删除了整个 `workspace.metadata.typos` 配置段，仓库不再维护 `typos` 白名单或标识符例外。
- 开发文档已同步：[`README.md`](/Users/hufei/RustroverProjects/onetcli/README.md)、[`README_CN.md`](/Users/hufei/RustroverProjects/onetcli/README_CN.md)、[`CLAUDE.md`](/Users/hufei/RustroverProjects/onetcli/CLAUDE.md) 均已删除 `typos` 开发命令，避免文档与 CI 不一致。
- 本地验证有效：`cargo metadata --format-version 1 --no-deps >/dev/null` 通过，且针对核心入口文件的 `typos` 搜索结果为 0。

### 剩余风险
- 如果后续仍希望保留拼写检查能力，需要重新选择替代工具或恢复新的检查链路；当前仓库已完全不再依赖 `typos`。

---

## 审查报告（encourage-unused-imports 实现）
生成时间：2026-03-26 10:19:36 +0800

### 需求完整性检查
- 目标明确：修复 Linux / Windows CI 在 `main/src/encourage.rs` 上的 unused imports 失败。
- 范围明确：只清理 `encourage.rs` 顶部的遗留导入，不改渲染行为。
- 交付物明确：代码修复、上下文摘要、操作日志、本地验证和审查报告均已补齐。
- 风险与依赖明确：`gpui` 某些链式方法依赖 trait 导入，因此必须以编译结果校验是否误删必要 trait。

### 技术维度评分
- 代码质量：97/100
- 测试覆盖：90/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：97/100
- 风险评估：94/100

### 综合评分
- 96/100
- 建议：通过

### 结论
- 遗留导入已清理：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 删除了未使用的 `InteractiveElement`、`StatefulInteractiveElement`、`Window`、`TabContent`、`TabContentEvent`。
- 必要 trait 仍保留：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 继续保留 `ParentElement`、`Styled`、`IntoElement`、`StyledImage` 等当前渲染链真实依赖的导入。
- 修复方式符合现有模式：对比 [`setting_tab.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/setting_tab.rs) 与 [`home_tab.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/home_tab.rs) 后，本次仅让 `encourage.rs` 的导入与其“纯渲染 helper”职责重新一致。
- 本地验证有效：`cargo check -p main --all-targets` 已通过。

### 剩余风险
- 当前只验证了本地 `main` crate 的全 target 编译；如果远端 CI 还存在缓存或其他分支差异，需要以最新提交重新跑一次流程确认。

---

## 审查报告（ci-followup-build-ssh 实现）
生成时间：2026-03-26 10:40:21 +0800

### 需求完整性检查
- 目标明确：修复后续 CI 暴露的 `build.rs` Clippy 问题和 `ssh.rs` Windows 测试告警。
- 范围明确：只处理 `crates/core/build.rs`、`main/build.rs`、`crates/ssh/src/ssh.rs` 这三处。
- 交付物明确：代码修复、上下文摘要、操作日志、本地验证和审查报告均已更新。
- 风险与依赖明确：完整 Clippy 流程继续暴露出更多历史问题，因此本次不能宣称全量 lint 已清零。

### 技术维度评分
- 代码质量：96/100
- 测试覆盖：87/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：89/100

### 综合评分
- 92/100
- 建议：通过

### 结论
- build script Clippy 问题已修复：[`crates/core/build.rs`](/Users/hufei/RustroverProjects/onetcli/crates/core/build.rs) 与 [`main/build.rs`](/Users/hufei/RustroverProjects/onetcli/main/build.rs) 已把嵌套 `if` 改为 let-chain，不再触发 `collapsible_if`。
- Windows 测试下的 unused/dead code 已修复：[`crates/ssh/src/ssh.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ssh/src/ssh.rs) 将 `Mutex`、`OnceLock` 和 `test_auth_failure_messages` 收紧到 `#[cfg(unix)]`，避免在 Windows test target 下变成未使用。
- 同文件额外 Clippy 问题已顺手修复：[`crates/ssh/src/ssh.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ssh/src/ssh.rs) 的 `hash_alg.clone()` 已移除，消除 `clone_on_copy`。
- 本地验证有效：`cargo test -p ssh --lib` 已通过。

### 剩余风险
- `cargo clippy -p one-core -p main --all-targets -- -D warnings` 继续报出 `crates/one_ui` 与 `crates/core` 中 100+ 个既有 Clippy 问题，例如 `derivable_impls`、`unnecessary_unwrap`、`needless_lifetimes`、`unnecessary_to_owned`、`redundant_closure`、`manual_contains`、`too_many_arguments` 等。当前 release/tag 若重新触发，仍会被这些后续问题挡住。

---

## 审查报告（db-connection-form-ssh-ssl-fixed 实现）
生成时间：2026-03-25 21:57:00 +0800

### 需求完整性检查
- 目标明确：将数据库连接表单中的 `ssl` 与 `ssh` 页签改成固定代码渲染，并用复选框控制整块启用。
- 范围明确：改动限定在 `crates/db_view/src/common/db_connection_form.rs` 的渲染、辅助逻辑和单元测试，不触碰存储结构。
- 交付物明确：代码修改、上下文摘要、操作日志、本地测试和审查报告均已落地。
- 风险与依赖明确：依赖既有 `extra_params` 键名和 `ssh_form_window.rs` 交互模式；ClickHouse `ssl` 页签本次保持通用渲染，属于刻意收敛范围。

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：89/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- `ssh` 页签已改为固定代码渲染：[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L2115) 新增 `render_ssh_tab_content`，使用复选框控制整块启用，并用单选控制密码、私钥、agent 三种认证输入联动。
- `ssl` 页签已改为固定代码渲染：[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L2222) 新增 `render_ssl_tab_content`，对 MySQL/PostgreSQL/MSSQL 分别按既有字段语义做启用控制。
- 通用状态链路保持不变：[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L1819) 继续保留标准页签渲染和字段状态容器，专用页签仍通过原 `set_field_value/get_field_value/build_connection/load_connection` 工作。
- SSH agent 校验已对齐后端语义：[`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L924) 与 [`db_connection_form.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/common/db_connection_form.rs#L1390) 通过纯函数统一必填判断，agent 模式不再错误要求密码。
- 本地验证有效：`CLANG_MODULE_CACHE_PATH=/tmp/clang-cache cargo test -p db_view --lib db_connection_form` 与 `cargo check -p db_view` 均已通过。

### 剩余风险
- 目前覆盖的是纯函数和字段定义层测试，未做 UI 交互快照或人工点击回归，布局细节仍建议你本地实际点一下表单确认观感。
- ClickHouse 的 `ssl` 页签仍沿用原通用渲染，因为本次需求和参考模式主要针对 MySQL/PostgreSQL/MSSQL 的 SSL 语义与 SSH 联动场景。

---

## 审查补充（home-encourage-tab 二次视觉调整）
生成时间：2026-03-25 23:33:00 +0800

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：80/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：97/100
- 风险评估：88/100

### 综合评分
- 92/100
- 建议：通过

### 结论
- 支持页签布局已重构为居中分区卡片：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 现在按“顶部说明卡片 + 中部支付卡片区 + 底部辅助支持卡片”三段展示，不再像截图那样散在左上角。
- 支付卡片层级已增强：[`encourage.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/encourage.rs) 为每个赞赏方式增加外层容器、统一间距和标题行图标，二维码区域更集中。
- 图标风格已统一：页签图标改为星标，[`home_tab.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/home_tab.rs) 中首页入口图标也同步从心形改成了星标，辅助支持区补了 `CircleCheck`、`GitHub`、`ExternalLink` 图标。
- 局部编译筛查有效：`cargo check -p main --keep-going --message-format short 2>&1 | rg 'main/src/(encourage|home_tab)\\.rs|error\\['` 无输出，说明这次视觉调整未给目标文件引入新报错。

---

## 审查报告（superpowers-install 实现）
生成时间：2026-03-26 11:05:28 +0800

### 需求完整性检查
- 目标明确：按 `obra/superpowers` 官方 `.codex/INSTALL.md` 在本机启用 Codex 原生技能发现。
- 范围明确：仅涉及用户主目录下的 clone、目录创建、软链接创建和旧 bootstrap 检查，不修改 onetcli 业务代码。
- 交付物明确：上下文摘要、操作日志、验证报告和本地安装结果均已落地。
- 风险与依赖明确：依赖 `git` 和用户主目录写权限；技能发现最终还需要重启 Codex。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：88/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：95/100
- 风险评估：92/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- 官方安装步骤已完整执行：`/Users/hufei/.codex/superpowers` 已成功克隆 superpowers 仓库，`/Users/hufei/.agents/skills/superpowers` 已建立为指向 `/Users/hufei/.codex/superpowers/skills` 的软链接。
- 迁移检查已完成：`/Users/hufei/.codex/AGENTS.md` 当前为空文件，不存在文档中提到的 `superpowers-codex bootstrap` 旧块，因此无需清理。
- 本地验证有效：`ls -la /Users/hufei/.agents/skills/superpowers` 已确认软链接存在且目标正确，`ls -la /Users/hufei/.codex/superpowers/skills` 已确认技能目录实际存在。
- 剩余人工步骤明确：仍需退出并重新启动 Codex CLI，才能让当前会话发现新安装的 superpowers 技能。

### 剩余风险
- 当前会话无法替代一次真正的 Codex 重启，因此“技能已被新会话识别”这一步只能在你重启 CLI 后做最终确认。

---

## 审查报告（window-not-found-fix 实现）
生成时间：2026-03-26 11:36:00 +0800

### 需求完整性检查
- 目标明确：修复关闭主窗口时出现的 `gpui::window: window not found` 生命周期竞态。
- 范围明确：仅调整应用退出策略，不改业务页签、更新逻辑或数据结构。
- 交付物明确：代码修复、上下文摘要、操作日志和本地验证结果均已落地。
- 风险与依赖明确：依赖 `gpui::QuitMode::LastWindowClosed`；图形界面层面的最终效果仍需人工冒烟确认。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：82/100
- 规范遵循：97/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：97/100
- 风险评估：90/100

### 综合评分
- 93/100
- 建议：通过

### 结论
- 应用退出策略已切换到官方 quit mode：[`main.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/main.rs#L29) 现在通过 `Application::with_quit_mode(QuitMode::LastWindowClosed)` 声明“最后一个窗口关闭时自动退出”。
- 手写 release 退出监听已移除：[`onetcli_app.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/onetcli_app.rs#L350) 不再在主窗口释放阶段调用 `cx.quit()`，减少窗口释放后与平台尾随事件交错的竞态面。
- 既有退出收尾链路仍保留：[`onetcli_app.rs`](/Users/hufei/RustroverProjects/onetcli/main/src/onetcli_app.rs#L350) 后续的 `on_app_quit` 逻辑未变，标签状态保存行为仍由原实现负责。
- 本地编译验证有效：`cargo check -p main` 已通过。

### 剩余风险
- 当前没有自动化 GUI 测试覆盖“关闭主窗口”这一交互，仍需在 macOS 图形环境人工确认一次日志是否消失。
- 如果修复后仍在关闭窗口瞬间看到同样日志，则问题可能残留在 `gpui` 上游对平台尾随事件的处理，需要进一步向框架层收敛。

---

## 审查报告（ui-main-safe-merge 实现）
生成时间：2026-03-26 12:13:00 +0800

### 需求完整性检查
- 目标明确：仅回迁 main 中可直接合并的 UI crates 提交，不能直接合并的忽略。
- 范围明确：本次仅涉及 `crates/ui` 内部低风险补丁，不扩展到 `table`、`dialog`、`time picker`、`WASM` 等高风险链路。
- 交付物明确：已完成提交筛选、临时 worktree 演练、当前分支回迁和本地编译验证。
- 风险与依赖明确：高风险提交已明确跳过，验证依赖 `CLANG_MODULE_CACHE_PATH=/tmp/clang-cache`。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：88/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：95/100
- 风险评估：95/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 已成功回迁 7 个低风险 UI 提交：tree 聚焦、窗口阴影尺寸、Linux 最大化 resize 修复、sheet 拖动修复、通知中键关闭、按钮标签容器适配、sheet 重复打开焦点恢复。
- 临时 worktree 演练已证明这 7 个提交可以直接 `cherry-pick`，当前 `dev` 上也已成功应用，无冲突残留。
- 本地验证已通过：`env CLANG_MODULE_CACHE_PATH=/tmp/clang-cache cargo check -p main` 成功完成。
- 高风险提交已按要求忽略：`table` 重构、`dialog/alert_dialog`、`input/text/highlighter` 大改、`time picker` 移除、`WASM/gpui_platform` 链路均未引入。

### 剩余风险
- 仍有大量 main 的 UI 优化未回迁，后续若继续同步，需要按主题分批处理。
- 本次没有做 GUI 交互级自动化验证，涉及视觉和交互的细节仍建议后续图形环境冒烟一次。

---

## 审查报告（ui-main-conflict-merge 实现）
生成时间：2026-03-26 13:38:12 +0800

### 需求完整性检查
- 目标明确：把 main 的 `editor: Improve highlighting performance (#2128)` 与 `theme: Update input background to match Shadcn style. (#2135)` 迁入当前 `dev`，并处理真实冲突。
- 范围明确：仅修改 `crates/ui` 内与高亮器、输入样式相关的文件，不扩展到 `table`、`dialog`、`time picker` 消费方接口。
- 交付物明确：代码迁移、上下文摘要、操作日志、验证报告和本地编译验证结果均已落地。
- 风险与依赖明确：`mix_oklab` 与 `wasm_stub` 在当前分支缺失，已做兼容替换或显式不引入。

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：86/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：94/100
- 风险评估：91/100

### 综合评分
- 93/100
- 建议：通过

### 结论
- `#2128` 已完成手工冲突迁移：[`highlighter.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/highlighter/highlighter.rs) 现在支持同步解析超时、注入层预计算与后台解析结果回填；[`mode.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/input/mode.rs) 与 [`state.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/input/state.rs) 已接入后台解析派发；[`element.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/input/element.rs) 已按可见连续段批量刷新高亮并跳过超长行。
- `#2135` 已完成手工冲突迁移：[`theme/mod.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/theme/mod.rs) 新增 `input_background()` 并作为编辑器背景回退；[`input.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/input/input.rs)、[`select.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/select.rs)、[`date_picker.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/time/date_picker.rs)、[`otp_input.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/input/otp_input.rs) 等输入类组件已统一改走 `input_style`。
- 本地集成验证已通过：`env CLANG_MODULE_CACHE_PATH=/tmp/clang-cache cargo check -p main` 成功完成，说明当前 `dev` 对这两笔优化的迁移在编译层面成立。
- 兼容性处理已收敛：由于当前分支没有 `mix_oklab`，[`theme/mod.rs`](/Users/hufei/RustroverProjects/onetcli/crates/ui/src/theme/mod.rs) 使用现有 `mix` 近似替代；由于当前分支没有 `wasm_stub` 链路，本次仅迁入 native 可用的高亮性能优化。

### 剩余风险
- 当前没有 GUI 自动化或人工冒烟结果，深色模式输入背景与上游 main 的视觉细节可能仍有轻微偏差。
- 后台解析优化只验证了编译通过，超大文件编辑场景仍建议后续在图形环境实际输入一次确认卡顿改善是否符合预期。


---

## 审查报告（terminal-command-scroll-bottom 实现）
生成时间：2026-03-26 17:17:11 +0800

### 需求完整性检查
- 目标明确：修复 `view.rs` 中输入命令后应滚动到底部的行为。
- 范围明确：仅修改 `crates/terminal_view/src/view.rs` 的输入滚动协调逻辑和文件内测试。
- 交付物明确：代码修复、上下文摘要、操作日志、本地测试结果均已落地。
- 风险与依赖明确：核心风险是误影响其它滚动路径，本次通过最小改动避免扩散。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：92/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- `crates/terminal_view/src/view.rs` 新增了 `should_scroll_to_bottom_on_user_input`，在用户输入前统一清理陈旧的 `future_display_offset`。
- `write_to_pty` 现在会先取消待提交滚动，再在当前视图确实离开底部时调用既有的 `scroll_display(Bottom)`，保证输入命令后稳定停留在底部。
- 新增两条文件内回归测试，覆盖“当前已在底部但存在待提交偏移”和“当前离底部且存在待提交偏移”两个场景。
- 本地验证已完成：`cargo test -p terminal_view user_input_scroll --lib` 先失败后通过，`cargo test -p terminal_view --lib` 最终全部通过。

### 剩余风险
- 当前仍缺少图形界面层面的自动化冒烟，真实拖动滚动条后立刻输入命令的交互建议后续在 GUI 环境再确认一次。
- 现有验证集中在单元测试层，尚未覆盖鼠标拖动、滚轮和 ALT_SCREEN 混合操作的端到端路径。

---

## 审查报告（db-view-data-grid-multi-delete 实现）
生成时间：2026-03-26 19:47:33 +0800

### 需求完整性检查
- 目标明确：修复 `db_view` 数据编辑 `data_grid` 中多选多行后点击删除只处理最后活动行的问题。
- 范围明确：仅修改 `crates/db_view/src/table_data/data_grid.rs` 的删除入口和文件内测试，不扩散到 `one_ui` 公共接口。
- 交付物明确：代码修复、上下文摘要、操作日志和本地测试结果均已落地。
- 风险与依赖明确：核心风险是新行真实删除引发索引漂移，本次通过降序删除规避。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：93/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：94/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- [`data_grid.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/table_data/data_grid.rs#L62) 新增 `collect_delete_row_indices`，负责把多选区映射为唯一行集合，并确保删除顺序为降序。
- [`data_grid.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/table_data/data_grid.rs#L1077) 的删除按钮入口现在优先读取 `state.selection().all_cells()`，不再只依赖最后活动单元格；没有多选区时仍回退到既有单选逻辑。
- [`data_grid.rs`](/Users/hufei/RustroverProjects/onetcli/crates/db_view/src/table_data/data_grid.rs#L2564) 新增 3 个回归测试，覆盖去重降序、空选区 fallback、显式选区优先级。
- 本地验证已完成：`cargo test -p db_view --lib` 通过，`db_view` 全量 200 个单元测试成功。

### 剩余风险
- 目前没有 GUI 层自动化用例直接覆盖“鼠标框选多行后点击删除”的交互路径，建议后续在图形环境补一次冒烟验证。
- 当前策略基于 `all_cells()` 展开矩形选区；在极大面积多列多行框选场景下会先遍历单元格再压缩到行，但作为点击删除前的低频动作，当前开销可接受。

---

## 审查报告（table_designer 字段排序 SQL 生成）
生成时间：2026-03-27 00:05:00 +0800

### 需求完整性检查
- 目标明确：排序字段后应生成 ALTER TABLE 语句
- 范围明确：限定 MySQL 插件 SQL 生成逻辑
- 交付物明确：最小补丁、单元测试、验证输出与日志
- 风险与依赖明确：其他数据库不处理列排序

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：92/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 验证结果
- 已执行：`cargo test -p db mysql::plugin::tests::`
  - 结果：通过（32 passed）
- LSP 诊断：未执行
  - 原因：rust-analyzer 在当前工具链不可用

### 结论
- MySQL `build_alter_table_sql` 已补齐列顺序差异处理，生成 `MODIFY COLUMN ... FIRST/AFTER`。
- 新增测试 `test_build_alter_table_sql_reorder_columns` 已通过，覆盖仅排序变化场景。

### 追加说明（修复排序与新增列冗余修改）
- 新增测试：`test_build_alter_table_sql_add_column_no_reorder`
- 已执行：`cargo test -p db mysql::plugin::tests::`
  - 结果：通过（33 passed）
- 行为更新：仅当“既有列相对顺序变化”时才生成排序 MODIFY，避免新增列导致的冗余修改

### 追加说明（扩展 SQL 生成测试集）
- 新增测试：`test_build_alter_table_sql_reorder_with_modify_column`
- 已执行：`cargo test -p db mysql::plugin::tests::`
  - 结果：通过（34 passed）

---

## 审查报告（全数据库 SQL 生成测试集扩展）
生成时间：2026-03-27 00:45:00 +0800

### 需求完整性检查
- 目标明确：覆盖所有数据库插件的 SQL 生成场景
- 范围明确：仅新增各插件测试用例
- 交付物明确：测试用例 + 本地验证记录

### 验证结果
- 已执行：`cargo test -p db postgresql::plugin::tests::`（31 passed）
- 已执行：`cargo test -p db mssql::plugin::tests::`（26 passed）
- 已执行：`cargo test -p db oracle::plugin::tests::`（26 passed）
- 已执行：`cargo test -p db sqlite::plugin::tests::`（21 passed）
- 已执行：`cargo test -p db clickhouse::plugin::tests::`（20 passed）
- LSP 诊断：未执行（rust-analyzer 不可用）

### 结论
- PostgreSQL：新增顺序变化无差异与默认/非空变更测试
- MSSQL：新增 ALTER COLUMN 与 UNIQUE INDEX 测试
- Oracle：新增 MODIFY 默认值/非空与 UNIQUE INDEX 测试
- SQLite：新增结构变更重建与顺序变化无差异测试
- ClickHouse：新增 MODIFY 类型与 ADD INDEX 测试

---

## 审查报告（SFTP 右键菜单补齐）
生成时间：2026-03-27 14:01:26 +0800

### 需求完整性检查
- 目标明确：补齐 SFTP 空白区域与 `..` 行的右键菜单
- 范围明确：覆盖 `crates/sftp_view` 与 `crates/terminal_view` 两套文件管理 UI
- 交付物明确：代码改动、文案补充、本地验证、操作日志与审查报告
- 风险与依赖明确：依赖既有 `ContextMenu`/`PopupMenu` 体系，GUI 交互缺少自动化冒烟

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：86/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：94/100
- 风险评估：89/100

### 综合评分
- 92/100
- 建议：通过

### 验证结果
- 已执行：`cargo check -p sftp_view -p terminal_view`
  - 结果：通过
- 已执行：`cargo test -p sftp_view -p terminal_view --lib --no-run`
  - 结果：通过
- 已执行：`cargo fmt --check`
  - 结果：失败（仓库内存在无关既有格式漂移）
- 已执行：`cargo fmt --all -- crates/sftp_view/src/file_list_panel.rs crates/terminal_view/src/sidebar/file_manager_panel.rs`
  - 结果：通过

### 结论
- 空白区域右键菜单已补齐，当前目录级操作可在空白区触发。
- `..` 行右键菜单已补齐，可直接执行进入上级目录等操作。
- 文件行新增 `occlude()` 避免父容器空白区菜单误命中。
- 已补充纯单测覆盖父目录推导逻辑，并修复本地单段相对路径的父目录边界。

---

## 审查报告（connection-restore 实现）
生成时间：2026-03-28 00:33:01 +0800

### 需求完整性检查
- 目标明确：退出时保存当前已打开的连接页，并在下次启动后提示是否恢复
- 范围明确：覆盖恢复快照模型、连接页最小状态导出、退出保存、首页恢复提示与按类型恢复执行
- 交付物明确：代码实现、上下文摘要、实施方案、操作日志、验证报告、本地验证结果
- 风险与依赖明确：启动提示依赖工作区和连接数据先完成加载；GUI 弹窗交互仍需桌面回归

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：89/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 验证结果
- 已执行：`cargo fmt --all`
  - 结果：通过
- 已执行：`cargo test -p one-core connection_restore -- --nocapture`
  - 结果：通过，3 个测试全部通过
- 已执行：`cargo check -p main`
  - 结果：通过

### 结论
- 恢复模型边界清晰：[`connection_restore.rs`](/usr/htdocs/onetcli/crates/core/src/connection_restore.rs) 独立维护连接恢复快照，避免把需求扩展成完整 tab builder 恢复。
- 连接页状态导出收敛：[`view.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/view.rs)、[`lib.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/lib.rs)、[`database_tab.rs`](/usr/htdocs/onetcli/crates/db_view/src/database_tab.rs)、[`redis_tab.rs`](/usr/htdocs/onetcli/crates/redis_view/src/redis_tab.rs)、[`mongo_tab.rs`](/usr/htdocs/onetcli/crates/mongodb_view/src/mongo_tab.rs) 现在只输出恢复所需的最小元数据。
- 启动提示链路已闭环：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs) 在连接与工作区均加载完成后弹出恢复对话框，并支持跳过或按勾选项恢复。
- 恢复类型不会漂移：[`home_tabs.rs`](/usr/htdocs/onetcli/main/src/home/home_tabs.rs) 新增按指定模式恢复数据库、Redis、MongoDB 的入口，不再被当前 `DatabaseOpenMode` 改写。
- 时序隐患已处理：[`home_tab.rs`](/usr/htdocs/onetcli/main/src/home_tab.rs) 对无效快照改为 `window.defer(...)` 后清理，避免在 `render()` 阶段直接改状态。

### 残余风险
- 恢复弹窗的勾选交互和多标签恢复顺序还没有做桌面手工回归。
- 快照文件解析失败时当前策略是忽略并告警，不会自动清理损坏文件。

---

## 审查补充（connection-restore 提示未出现修复）
生成时间：2026-03-28 00:44:22 +0800

### 结论
- 根因已确认：恢复提示缺失不是首页弹窗逻辑没走，而是 `connection_restore_state.json` 没有被稳定写出。
- 保存链路已收敛：[`tab_persistence.rs`](/usr/htdocs/onetcli/crates/core/src/tab_persistence.rs) 现在会在 `save_tab_state(...)` 内同步保存连接恢复快照。
- 退出保存已加固：[`onetcli_app.rs`](/usr/htdocs/onetcli/main/src/onetcli_app.rs) 的 `on_app_quit` 现在同步保存，不再依赖可能来不及执行的后台任务。
- 回归测试已补齐：[`tab_persistence.rs`](/usr/htdocs/onetcli/crates/core/src/tab_persistence.rs) 新增“保存标签状态时同步写入连接恢复快照”单测。

### 验证结果
- `cargo test -p one-core 保存标签状态时同步写入连接恢复快照 -- --nocapture`
  - 结果：通过
- `cargo check -p main`
  - 结果：通过

---

## 审查补充（Deepin 自动切换主题）
生成时间：2026-03-28 03:08:00 +0800

### 需求完整性检查
- 目标明确：排查 Deepin 下“自动切换主题”无效的真实原因，并修正取值来源
- 范围明确：仅修复主题来源与判定逻辑，不引入新的实时监听机制
- 交付物明确：代码修复、本地验证、交互式现场取证结论、操作日志

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：90/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：94/100
- 风险评估：92/100

### 综合评分
- 94/100
- 建议：通过

### 关键结论
- `gsettings` 不是当前 Deepin 会话里的可靠亮暗模式来源
- 会真实反映系统切换的是会话总线服务 `org.deepin.dde.Appearance1`
- 其中 `GlobalTheme` 与 `GtkTheme` 都能体现亮暗态，且实机切换时确实发生变化
- 当前实现已改为优先读取 `Appearance1`，并保留 `gsettings` 兜底

### 验证结果
- `cargo test -p main 自动切换 --bin onetcli -- --nocapture`
  - 结果：通过
- `cargo test -p main deepin_主题名可映射为亮暗模式 --bin onetcli -- --nocapture`
  - 结果：通过
- `cargo check -p main`
  - 结果：通过

### 残余风险
- 当前仍未做“应用持续前台时系统切换后秒级自动刷新”的额外增强；这是按当前需求刻意不实现
- 运行期依赖系统存在 `gdbus` 命令；若极端环境缺失，则会回退到原有 `gsettings` / GPUI 外观判断

---

## 审查补充（SFTP 上传下载模式收口）
生成时间：2026-03-28 03:32:30 +0800

### 需求完整性检查
- 目标明确：区分面板模式与独立页面模式的上传下载入口
- 范围明确：仅调整 `sftp_view` 的右键菜单、事件语义与右键选区行为，不改终端侧边栏面板
- 交付物明确：代码修改、上下文摘要、操作日志、本地验证结果
- 风险与依赖明确：主要风险是右键命中项与当前选区错位导致误操作

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：89/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：92/100

### 综合评分
- 94/100
- 建议：通过

### 关键结论
- 独立页面远程侧右键已不再保留系统选择器上传入口，上传职责收口到本地侧
- 独立页面本地侧继续复用 `upload_selected`，上传目标仍是远程当前路径
- 远程侧下载逻辑未被重写，仍保持下载到本地当前目录
- 右键命中项现在会同步选区，可避免下载/删除等动作作用到旧选区

### 验证结果
- `cargo fmt --all -- crates/sftp_view/src/file_list_panel.rs crates/sftp_view/src/context_menu_handler.rs`
  - 结果：通过
- `cargo check -p sftp_view`
  - 结果：通过
- `cargo test -p sftp_view context_selection_ --lib -- --nocapture`
  - 结果：通过
- `cargo test -p sftp_view parent_path_ --lib -- --nocapture`
  - 结果：通过

### 残余风险
- 右键同步选区的体验已通过单测覆盖核心规则，但仍需要桌面实测确认和多选习惯一致
- 目前没有新增 GUI 自动化测试，实际菜单可发现性仍需人工回归

---

## 审查补充（SFTP 右键菜单竞争修复）
生成时间：2026-03-28 03:43:00 +0800

### 关键结论
- 问题根因不在上传/下载业务逻辑，而在通用 `context_menu` 组件未阻断父级菜单传播
- 已在组件层加入 `cx.stop_propagation()`，避免文件项菜单与空白区菜单竞争同一次右键事件
- 该修复对独立页面 `sftp_view` 和终端右侧 `file_manager_panel` 同时生效

### 验证结果
- `cargo fmt --all -- crates/ui/src/menu/context_menu.rs`
  - 结果：通过
- `cargo check -p sftp_view -p terminal_view`
  - 结果：通过

### 残余风险
- 仍需桌面手工确认不同区域嵌套右键菜单的最终体验，但编译链路已验证通过

---

## 审查补充（SFTP 文件行命中区域修正）
生成时间：2026-03-28 03:49:00 +0800

### 关键结论
- 菜单错位的真实根因是文件行点击热区太窄，而不是上传下载业务逻辑本身
- 通过把文件行与 `..` 行的外层容器和行内容都扩展到整行宽度，远程文件项菜单会稳定显示“下载”，本地文件项菜单也会稳定显示并执行“上传”
- 通用 `context_menu` 组件已恢复原状，避免对其他模块产生额外副作用

### 验证结果
- `cargo fmt --all -- crates/ui/src/menu/context_menu.rs crates/sftp_view/src/file_list_panel.rs`
  - 结果：通过
- `cargo check -p sftp_view -p terminal_view`
  - 结果：通过

### 残余风险
- 仍需你在实际界面上点测“列右侧空白处右键”的场景，确认菜单完全收口到文件项级别
## 审查补充（Windows 鼠标拖动与拖动排序修复）
生成时间：2026-03-28 20:44:33 +08:00

### 需求完整性检查
- 目标明确：修复 Windows x64 下主窗口鼠标拖动失效，并恢复 tab 拖动排序。
- 范围明确：仅调整主窗口 `TabContainer` 的拖窗热区与平台分支，不改动入口窗口配置、不重写标题栏体系。
- 交付物明确：代码修复、最小单测、上下文摘要、操作日志、验证记录。
- 依赖与风险明确：依赖 `gpui` 的 `WindowControlArea` 命中逻辑；主要风险是修复拖窗时误伤 tab 点击、关闭与排序交互。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：89/100
- 规范遵循：93/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：92/100

### 综合评分
- 93/100
- 建议：通过

### 关键结论
- 根因不在“Windows 完全不支持鼠标拖动”，而在于 `crates/core/src/tab_container.rs` 把整个 `#tabs` 滚动容器声明成了 `WindowControlArea::Drag`。
- 在 Windows 上，这会让系统标题栏 hit-test 抢走 tab 的鼠标事件，直接破坏 tab 拖动排序，也会让拖窗与 tab 交互纠缠在一起。
- 修复方案把 Windows 的拖窗能力收敛成一个独立热区 `tab-bar-drag-spacer`，同时保留 Linux/macOS 原有的手动 `start_window_move()` 链路。
- 这样可以同时满足两点：
  - Windows 仍然有稳定可拖窗区域
  - tab 本体重新拿回点击、关闭、拖动排序事件

### 验证结果
- `& 'C:\Users\hoping\.cargo\bin\cargo.exe' fmt --all -- crates/core/src/tab_container.rs`
  - 结果：通过
- `& 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p one-core`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' test -p one-core tab_container::tests --lib -- --nocapture`
  - 结果：通过，2 个新增单测全部通过

### 审查说明
- 默认增量编译下，`cargo test` 曾因 `os error 112` 失败；切换为 `CARGO_INCREMENTAL=0` 后测试执行成功。
- 因此本次验证没有保留“磁盘空间不足导致测试未跑”的缺口，已经通过本地 AI 自动执行完成补偿验证。

### 残余风险
- 仍需在 Windows GUI 实机验证 tab 排序、空白热区拖窗、关闭按钮和下拉菜单按钮的最终交互手感。
- 当前未新增端到端 GUI 自动化，桌面级交互回归仍依赖人工点击确认。

## 审查补充（Windows 鼠标拖动与拖动排序修复第二轮）
生成时间：2026-03-28 21:26:12 +08:00

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：89/100
- 规范遵循：94/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：95/100
- 风险评估：92/100

### 综合评分
- 94/100
- 建议：通过

### 新增关键结论
- 第一轮方案只解决了“不要让整个 `#tabs` 容器变成 Windows 标题栏拖窗区”，但没有解决“Windows 下 tab 自己的拖拽起手仍被多余鼠标拦截”的问题。
- 第二轮按平台拆开了这组前置鼠标拦截逻辑：
  - Linux/macOS 继续保留，避免父级手动拖窗抢事件；
  - Windows 取消该拦截，恢复 tab 自身 `on_drag(...)`。
- 同时新增了 `#tabs` 内联空白拖窗区与加宽后的右侧兜底拖窗区，使 Windows 不再只依赖一个过窄热区。

### 第二轮验证结果
- `& 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p one-core`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' test -p one-core tab_container::tests --lib -- --nocapture`
  - 结果：通过

### 关于窗口句柄错误的审查意见
- 当前证据更支持“异步窗口生命周期日志噪音”而不是“本次拖拽修复直接引入的功能性错误”。
- 若用户后续确认拖拽修复后仍稳定出现该日志，再建议单独开一轮针对 `update_window/read` 生命周期的收口修复，避免把两个问题混在一次改动里。

## 审查补充（Windows 窗口句柄错误收敛）
生成时间：2026-03-28 21:50:10 +08:00

### 需求完整性检查
- 目标明确：处理 Windows 下关窗尾声出现的 `window not found`、`0x80040102`、`0x80070578` 日志。
- 范围明确：仅收敛 `vendor/zed/crates/gpui/src/platform/windows` 的生命周期与句柄清理，不改业务层拖拽逻辑。
- 交付物明确：代码修复、上下文摘要、操作日志、验证报告。
- 依赖与风险明确：依赖 GPUI 现有 `Callbacks` 与 `WM_DESTROY` 生命周期；主要风险是过度吞错导致真实 Windows 平台错误被隐藏。

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：81/100
- 规范遵循：92/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：94/100
- 风险评估：88/100

### 综合评分
- 90/100
- 建议：通过

### 关键结论
- 根因不是业务层拖拽代码继续直接调用失效窗口，而是 Windows 平台层在 `WM_DESTROY` 之后仍可能保留回调与句柄清理动作。
- 本次修复把收口点放在 Windows 平台层：
  - 先在 `WM_DESTROY` 断开会继续回到 GPUI 实体的剩余回调
  - 再对 `RevokeDragDrop` / `DestroyWindow` 只忽略已确认的无效句柄错误
  - 对 `WM_NCHITTEST` / `WM_NCMOUSEMOVE` 的 Win32 API 调用加有效句柄守卫
- 这样既能降低关窗尾声的日志噪音，又不会改写 GPUI 全局的 `window not found` 错误语义。

### 验证结果
- `& 'C:\Users\hoping\.cargo\bin\rustfmt.exe' --edition 2024 vendor/zed/crates/gpui/src/platform/windows/util.rs vendor/zed/crates/gpui/src/platform/windows/window.rs vendor/zed/crates/gpui/src/platform/windows/events.rs`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p one-core`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p main`
  - 结果：失败，受环境缺少 `cmake` / `nasm` 影响
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p gpui`
  - 结果：失败，受当前会话无法访问 `static.crates.io` 影响

### 残余风险
- 仍需在 Windows 实机复现“关窗、拖窗、拖动排序后无上述日志”才能确认日志完全收口。
- 这次只忽略了已确认的两类无效句柄错误；如果后续还有其他关窗尾声错误码，仍需要继续补充证据后再处理。

## 主窗口状态恢复修复审查
审查时间：2026-03-28 22:24:00 +08:00

### 需求完整性检查
- 目标明确：关闭应用时保存主窗口尺寸与状态，并在下次启动时恢复。
- 范围明确：仅处理主窗口，不混入弹窗或其他子窗口状态。
- 交付物明确：代码修复、上下文摘要、操作日志、验证报告。
- 依赖与风险明确：依赖 GPUI `WindowBounds` 与 `observe_window_bounds(...)`；主要风险是缺少 Windows GUI 实机验证与全量编译验证。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：76/100
- 规范遵循：93/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：94/100
- 风险评估：84/100

### 综合评分
- 89/100
- 建议：需讨论

### 关键结论
- 根因已经明确：旧代码没有把主窗口状态接入 `AppSettings`，且启动阶段始终使用固定居中窗口 bounds，导致“保存”和“恢复”链路事实上都不完整。
- 本次修复沿用项目现有模式：
  - 通过 `AppSettings` 保存 `WindowBounds`
  - 通过 `observe_window_bounds(...)` 在运行时增量写盘
  - 通过 `main.rs` 在启动时恢复 `WindowBounds`
- 方案与 GPUI 原生语义一致，不需要自造平台特判或第二套配置文件。

### 验证结果
- `& 'C:\Users\hoping\.cargo\bin\rustfmt.exe' --edition 2024 main/src/setting_tab.rs main/src/main.rs main/src/onetcli_app.rs`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' check -p main`
  - 结果：失败，受环境缺少 `cmake` / `nasm` 影响
- 静态审查结果：
  - `main/src/main.rs` 已改为消费保存的 `WindowBounds`
  - `main/src/onetcli_app.rs` 已注册窗口 bounds 监听保存
  - `main/src/setting_tab.rs` 已补齐序列化结构与恢复方法

### 残余风险
- 还需在 Windows 实机验证最大化、窗口化、关闭后重启是否都能恢复到预期状态。
- 如果用户切换显示器布局，当前未额外处理越界坐标回正；这不是本次链路缺失的主因，但后续可能仍需补充体验优化。

## 主窗口状态保存防抖修复审查
审查时间：2026-03-28 23:02:00 +08:00

### 技术维度评分
- 代码质量：93/100
- 测试覆盖：74/100
- 规范遵循：93/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：95/100
- 风险评估：86/100

### 综合评分
- 89/100
- 建议：需讨论

### 关键结论
- 这次回归不是 Windows 拖拽命中逻辑失效，而是窗口状态保存被错误地放在高频 bounds 回调里同步写盘，导致窗口拖动过程卡顿到接近不可用。
- 修复后改为：
  - bounds 回调只更新内存中的最新窗口状态
  - 通过 `Debouncer` 延迟写盘
  - 应用退出时再兜底写一次全局设置
- 该方案与项目现有标签布局延迟保存模式一致，回归面比继续改平台拖拽逻辑更小。

## 主窗口拖动回归二次修复审查
审查时间：2026-03-28 23:18:00 +08:00

### 综合评分
- 90/100
- 建议：通过

### 关键结论
- 进一步核对后，真正会在拖动过程中高频触发的回归点是 `AppSettings::global_mut(...)` 带来的全局观察者通知，而不只是同步写盘。
- 修复后把高频路径收敛为纯本地缓存更新；只有在防抖到期或实体释放时，才把缓存刷入全局设置并落盘。
- 该修复比继续改 `TabContainer` 的 Windows 命中区更聚焦，也更符合“窗口状态恢复不应影响拖动体验”的需求边界。

## 启动恢复窗口居中修复审查
审查时间：2026-03-29 04:32:25 +08:00

### 需求完整性检查
- 目标明确：修复启动恢复窗口可能超出屏幕底部的问题，并确保异常恢复位置回到应用中间。
- 范围明确：仅处理主窗口默认启动与已保存窗口恢复，不涉及弹窗或运行时拖拽逻辑。
- 交付物明确：代码修复、上下文摘要、操作日志、验证报告。
- 审查要点明确：恢复位置是否基于 `visible_bounds()`，默认尺寸是否避开底部不可见区域，越界与超大尺寸是否都有测试覆盖。

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：91/100
- 规范遵循：94/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 关键结论
- 修复不再只依赖整块屏幕 `bounds()`，而是统一切到 `visible_bounds()`，这直接覆盖了底部任务栏导致的窗口超屏问题。
- 代码同时补齐了两条遗漏路径：
  - 已保存窗口位置越界时，裁剪尺寸并重新居中
  - 没有有效恢复值时，默认窗口尺寸和默认居中也避开不可见区域
- 方案保持了原有 `AppSettings -> WindowBounds` 恢复架构，没有引入平台特判或第二套窗口状态系统。

### 验证结果
- `& 'C:\Users\hoping\.cargo\bin\rustfmt.exe' --edition 2024 main/src/setting_tab.rs main/src/main.rs`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' test -p main 主窗口 -- --nocapture`
  - 结果：通过，5 个主窗口相关测试全部通过

### 残余风险
- 当前验证仍以单元测试为主，尚未记录真实 Windows 多显示器环境下的人工启动截图。
- 仓库里仍有与本次改动无关的 warning；虽然不影响本次修复结论，但后续可以单独清理。

## 恢复连接弹窗布局修复审查
审查时间：2026-03-29 04:44:42 +08:00

### 需求完整性检查
- 目标明确：修复启动时“恢复连接”弹窗超出应用窗口的问题，并改善其鼠标拖动可用性。
- 范围明确：仅处理 `main/src/connection_restore.rs` 对话框的布局与可拖动标题区，不触碰恢复业务逻辑。
- 交付物明确：代码修复、上下文摘要、操作日志、验证报告。
- 审查要点明确：是否按当前 viewport 自适应尺寸和偏移，是否限制列表高度，是否保留现有恢复确认流程。

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：89/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：95/100
- 风险评估：90/100

### 综合评分
- 94/100
- 建议：通过

### 关键结论
- 已确认此前修错层级：真正越界的是启动时的恢复连接对话框，而不是主窗口恢复尺寸。
- 本次修复把恢复对话框的宽度、顶部偏移和列表高度全部切换为基于 `window.viewport_size()` 的运行时计算，避免继续依赖通用 `Dialog` 的固定 `360px` 高度假设。
- 标题区改为更明显的双行头部，在不改底层拖动逻辑的前提下，提高了用户对可拖动区域的感知。

### 验证结果
- `& 'C:\Users\hoping\.cargo\bin\rustfmt.exe' --edition 2024 main/src/connection_restore.rs`
  - 结果：通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' test -p main connection_restore -- --nocapture`
  - 结果：通过，2 个恢复弹窗布局测试全部通过
- `$env:CARGO_INCREMENTAL='0'; & 'C:\Users\hoping\.cargo\bin\cargo.exe' test -p main 主窗口 -- --nocapture`
  - 结果：通过，7 个相关测试全部通过

### 残余风险
- 当前验证仍是纯逻辑和单元测试，尚未附带真实 Windows 启动场景截图。
- 如果未来恢复弹窗主体内容继续扩展，高度预估常量仍需同步调整或进一步抽象。

## 恢复连接弹窗 popup 窗口迁移审查
审查时间：2026-03-29 05:04:03 +08:00

### 需求完整性检查
- 目标明确：修复启动时恢复连接弹窗拖动困难、拖动卡顿，并继续确保其不会掉到屏幕外。
- 范围明确：仅处理 `main/src/connection_restore.rs` 与 `crates/core/src/popup_window.rs` 的弹窗形态、定位和关闭链路，不改恢复业务本身。
- 交付物明确：代码修复、上下文摘要、操作日志、验证报告。
- 审查要点明确：是否切换到独立 popup window，是否回到主窗口上下文执行恢复，是否保证关闭时清理恢复快照。

### 技术维度评分
- 代码质量：96/100
- 测试覆盖：90/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：91/100

### 综合评分
- 95/100
- 建议：通过

### 关键结论
- 这次修复不再停留在“给应用内 `Dialog` 补布局参数”，而是把恢复连接提示切换到仓库已有的独立 `popup window` 体系，直接对准用户反馈的“拖动不跟手、明显卡顿”问题。
- popup 通用层新增了自定义关闭回调能力，使右上角关闭按钮、Esc 和“跳过”都能收敛到“清理恢复快照”的同一语义，避免残留提示状态。
- 恢复动作通过 `GlobalMainWindowHandle + cx.update_window(...)` 回到主窗口执行，避免在 popup 视图上下文里错误调用 `update_in(...)`，也符合 `HomePage::restore_saved_connection_sessions(...)` 对主窗口 `Window` 的依赖。

### 验证结果
- `rustfmt --edition 2024 D:\usr\htdocs\onetcli\main\src\connection_restore.rs D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs`
  - 结果：通过
- `cargo test -p main connection_restore -- --nocapture`
  - 结果：通过，2 个恢复弹窗相关测试全部通过
- `cargo test -p main 主窗口 -- --nocapture`
  - 结果：通过，7 个主窗口与恢复弹窗相关测试全部通过

### 残余风险
- 当前仍缺少真实 Windows 启动场景下的拖动录屏或交互截图，体验结论主要基于代码路径和既有 popup 模式推断。
- `Esc` 关闭现在通过 popup 视图处理为“跳过恢复”；逻辑上已和其它关闭路径对齐，但后续若扩展更多关闭副作用，仍需继续保持幂等。

## 恢复连接弹窗拖拽命中区补齐审查
审查时间：2026-03-29 05:18:00 +08:00

### 技术维度评分
- 代码质量：97/100
- 测试覆盖：91/100
- 规范遵循：96/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：97/100
- 风险评估：92/100

### 综合评分
- 96/100
- 建议：通过

### 关键结论
- 最新用户反馈把问题进一步收窄到了“popup 可缩放但不可拖”，这说明窗口边框和尺寸策略没问题，真正缺的是顶部拖拽命中区。
- 对照仓库里其它正常 popup 后，确认恢复弹窗遗漏了统一的 `TitleBar::new()` 标题栏；补齐后可复用现有 `WindowControlArea::Drag` 与窗口控制区逻辑，而不是继续为单个弹窗单独写拖拽实现。

### 验证结果
- `rustfmt --edition 2024 D:\usr\htdocs\onetcli\main\src\connection_restore.rs`
  - 结果：通过
- `cargo test -p main connection_restore -- --nocapture`
  - 结果：通过
- `cargo test -p main 主窗口 -- --nocapture`
  - 结果：通过
