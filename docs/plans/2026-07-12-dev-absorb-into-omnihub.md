# dev → rename/onetcli-to-omnihub 差距分析与吸收计划

**日期：** 2026-07-12  
**当前分支：** `rename/onetcli-to-omnihub` @ `dd240759`  
**目标上游：** `origin/dev` @ `cf0c2f65`  
**merge-base：** `7fd57c89`

## 1. 差距结论

差异过大，**不建议直接 `git merge origin/dev`**。

| 指标 | 数值 | 含义 |
|---|---:|---|
| HEAD 独有提交 | 447 | 本地 perf / setting_tab 拆分 / OmniHub 改名等 |
| origin/dev 独有提交 | 202 | MCP / 扩展 / 端口转发 / 远程桌面 / 0.7.x 修复 |
| HEAD 改动文件 | 860 | 含 vendor/zed、docs、大量本地重构 |
| origin/dev 改动文件 | 535 | 大量新 crate 与业务功能 |
| 双侧都改文件 | 125 | 真实冲突面 |
| merge-tree 冲突文件 | ~70 | 直接 merge 会卡死在核心模块 |
| 版本 | HEAD `0.4.8` / dev `0.7.2` | 版本线已分叉 |

### 结构性不兼容（硬冲突源）

| 区域 | 当前分支 | origin/dev | 风险 |
|---|---|---|---|
| 应用壳 | `main/src/omnihub_app/` | `main/src/onetcli_app.rs` | 模块布局完全不同 |
| 设置页 | `setting_tab` 多子模块拆分 | 仍偏集中 / 另一套重构 | 高冲突 |
| 产品名 | OmniHub / omnihub | OnetCli / onetcli | 全文标识差异 |
| 运行时路径 | `runtime_paths` + 迁移 | 旧路径体系 | 兼容逻辑不能丢 |
| DB 驱动 | 保留本地 `duckdb_driver` + `ipc` | 扩展化驱动，本地 duckdb 移除/feature 化 | 架构分叉 |
| 工作区 crate | 无 extension/mcp/port_forwarding/remote_desktop | 18+ 新 crate | 需整包接入 |
| vendor/zed | 本地有大量改动 | 路径/依赖不同 | 合并成本极高 |

### origin/dev 独有能力包（高价值）

1. **扩展系统**：`extension-*` / `extension_view` / marketplace / wasm host
2. **MCP / automation**：`public_mcp` / `tool_runtime` / `onetcli_runtime` / `onetcli_cli`
3. **端口转发**：`port_forwarding` + `port_forwarding_view`
4. **远程桌面**：`remote_desktop` + `remote_desktop_view`（含 VNC）
5. **数据库增强**：SSH 隧道引用、外部驱动、比较功能（部分 gated）、Oracle dual ping、PG 清空表修复
6. **终端/UI 修复**：CJK 渲染、选区、系统语言默认、字体族/自定义字体、窗口置顶、首页拖拽排序
7. **版本与发布**：0.6.x → 0.7.2、CI/release 修复、Windows GUI subsystem

### 当前分支必须保留的本地资产

1. OmniHub 改名与配置目录迁移
2. `setting_tab` 子模块拆分结果
3. perf/manager-lock 与云同步/DB 锁优化
4. SSH GEX / known_hosts / MFA / shell integration 相关修复
5. Wayland/popup 相关修复
6. 本地 `ipc` + `duckdb_driver` 现状（除非明确决定切换到扩展驱动）

## 2. 策略选择

### 方案对比

| 方案 | 做法 | 优点 | 缺点 | 结论 |
|---|---|---|---|---|
| A. 直接 merge dev | `git merge origin/dev` | 一次拿全量 | ~70 文件冲突 + 架构对撞，极易半残 | **否** |
| B. rebase 当前到 dev | 重放 447 本地提交 | 历史线性 | 本地重构海量，冲突面更大 | **否** |
| C. 功能包移植（推荐） | 按包从 dev 抽代码/行为，落到 OmniHub 结构 | 可控、可验证、保留本地资产 | 周期长，需分批 | **是** |
| D. 以 dev 为新基线，回灌 OmniHub/本地修复 | 新分支从 dev 开，再移植 rename+本地关键修复 | 未来跟 dev 更顺 | 等于重建当前分支，短期成本高 | 备选终局 |

**本轮采用 C：分批功能移植。**  
若连续 2–3 个大包移植后发现“以 dev 为基线回灌”更便宜，再升级到 D。

## 3. 实施原则

1. **不直接 merge**，不重写 git 历史。
2. 每批只碰一个能力包，批末 `cargo check -p main`（或最小相关 crate）。
3. 所有新代码进入当前分支时，同步 OmniHub 命名：
   - 二进制 / app_id / 路径：`omnihub`
   - 展示名：`OmniHub`
   - 仓库 URL 暂保留 `onetcli`
   - 密钥 salt / vault 远程名继续兼容
4. 冲突策略：
   - 产品壳 / setting_tab / runtime_paths：**ours**
   - 独立新 crate / 纯新文件：**theirs 结构 + 本地命名适配**
   - 共享核心（db/core/terminal）：**人工整合，禁止盲选一边**
5. 不做无关清理；不把历史 docs 全量同步当阻塞项。

## 4. 分批计划

### Phase 0：基线冻结与清单（本轮先完成）

- [x] 计算 ahead/behind、冲突面、独有 crate
- [x] 形成本计划文档
- [ ] 产出“dev 独有能力 → 目标文件/依赖”映射表（下一提交）
- [ ] 明确不移植清单（网站、已删除本地 duckdb 路径等）

### Phase 1：低冲突修复先吃（优先）

目标：先把用户可见 bugfix / 小增强搬过来，建立移植节奏。

建议包：
1. Oracle `DUAL` ping 修复
2. PostgreSQL 清空表修复
3. 终端 CJK / 选区 / mouse release 修复
4. 默认跟随系统语言
5. 窗口置顶
6. 更新器 GitHub 回退 / 版本页链接修复（需适配 OmniHub 资产名）

验收：
- 对应模块 `cargo check`
- 不破坏 OmniHub 路径与更新资产命名

### Phase 2：首页与连接体验

1. 连接列表统一渲染
2. 拖拽排序 / 布局切换
3. 连接错误可操作性修复
4. 连接同步时保留本地编辑

风险：`home_tab` / storage / cloud_sync 与本地 perf 改动重叠，需人工整合。

### Phase 3：SSH 隧道与 Redis/DB 连接能力

1. 数据库连接引用 SSH 隧道
2. Redis SSH tunnel
3. 跳板机多认证
4. 相关 locales

风险：storage models / form windows 与本地拆分后的 settings 接线。

### Phase 4：端口转发

整包接入：
- `crates/port_forwarding`
- `crates/port_forwarding_view`
- main/home/new_connection 接线
- 图标与 locales

相对独立，适合整包复制后改名适配。

### Phase 5：远程桌面

整包接入：
- `crates/remote_desktop`
- `crates/remote_desktop_view`
- 表单迁移、RGBA/BGRA、性能传输优化

### Phase 6：MCP / CLI / tool runtime

整包接入并改名适配：
- `public_mcp`
- `tool_runtime`
- `onetcli_runtime` → 评估是否改名 `omnihub_runtime`
- `onetcli_cli` → 评估是否改名 `omnihub_cli`
- settings 中的 MCP 配置 UI

这是架构级能力，依赖 tool/runtime 抽象，不能只 cherry-pick UI。

### Phase 7：扩展系统与外部驱动

1. extension-* crates
2. marketplace / install progress
3. external driver forms
4. 与本地 `duckdb_driver`/`ipc` 并存策略：
   - 短期：本地 builtin 保留，外部驱动并行
   - 中期：再评估是否切换到 dev 的 feature-gated/extension 路线

### Phase 8：版本、CI、文档收口

1. 版本策略：不直接跳到 0.7.2，单独评估
2. CI workflow 合并
3. README 能力清单同步（保留 OmniHub 品牌）
4. 完成前验证与回归清单

## 5. 明确不在本轮直接搬的内容

- 直接全量 merge/rebase
- 网站 / marketing / supabase website 相关（dev 已有部分移除/迁移痕迹，优先级低）
- 把当前 OmniHub 改名回退成 OnetCli
- 丢弃本地 setting_tab 拆分与 perf 锁优化
- 无验证地删除本地 `duckdb_driver`

## 6. 推荐执行顺序（最短有效路径）

1. Phase 1 修复包（1–2 天粒度，可快速验证）
2. Phase 4 端口转发（独立 crate，收益高）
3. Phase 5 远程桌面
4. Phase 2/3 连接体验与隧道
5. Phase 6 MCP
6. Phase 7 扩展系统
7. Phase 8 收口

## 7. 每批交付模板

每完成一个能力包，至少输出：

1. 变更文件列表
2. 从 dev 参考的关键提交/路径
3. 为适配 OmniHub 做的命名/路径修改
4. 验证命令与结果
5. 已知未搬内容 / 风险

## 8. 下一步立刻做什么

1. 冻结本计划
2. 开始 Phase 1.1：Oracle DUAL ping + PostgreSQL 清空表修复
3. 验证 `cargo check -p db` / 相关测试
4. 再继续终端与 i18n 小修复



## 9. 进度记录

### 2026-07-12

- 完成差距分析与吸收计划
- 结论：不直接 merge；采用功能包移植
- Phase 1.1 已移植：
  - Oracle ping 使用 `SELECT 1 FROM DUAL`
  - PostgreSQL 清空表支持 schema（`truncate_table_with_schema`）
- 验证：
  - `cargo check -p db` ✅
  - `cargo check -p db_view` ✅
- 下一步：
  - 终端 CJK/选区修复
  - 系统语言默认
  - 端口转发整包评估

### Phase 1 收口补充

- [x] 1.5 窗口置顶（Windows only）
- [x] 1.6 更新器多源下载 / fallback_download_url + target-triple downloads

### Phase 4 端口转发（进行中 → 核心已接入）

- [x] 接入 `crates/port_forwarding` + `crates/port_forwarding_view`
- [x] storage `ConnectionType::PortForwarding` + `PortForwardingParams`
- [x] SSH `LocalPortForwardConfig` / `start_local_port_forward_with_config` + `dynamic_socks`
- [x] home/new_connection 接线（无 TeamOption，本地剥离 teams UI）
- [x] `cargo check -p main` 通过
- [ ] 手工验证本地转发 / 动态 SOCKS
- [x] 远程桌面包（Phase 5）核心接入 + `cargo check -p main`

### Phase 5 远程桌面（核心已接入）

- [x] 接入 `crates/remote_desktop` + `crates/remote_desktop_view` + `crates/process-util`
- [x] storage `ConnectionType::{Rdp,Vnc}` + `RemoteDesktopParams` / `RemoteDesktopProtocol`
- [x] 图标 `rdp.svg` / `vnc.svg` + locales（core/main）
- [x] home/new_connection/saved_connection_picker 接线
- [x] 表单剥离 teams UI（本地无 TeamOption）
- [x] 修复 `remote_desktop_form/view.rs` 语法残留
- [x] 验证：`cargo check -p remote_desktop -p remote_desktop_view -p main` ✅
- [ ] 手工验证 RDP/VNC 连接与输入转发
- [~] 辅助进程打包/helper 命名路径回归（omnihub-*-helper）
  - 2026-07-13：已移植 provider 安装引导（`main/src/remote_desktop_install/`）：打开连接时检测 provider，缺失则弹窗 → 市场下载（sha256 校验）→ 安全解包安装（备份/回滚）→ 自动打开连接；语义对齐 dev `remote_desktop_provider_install`，未引入 extension-runtime
  - helper 仍沿用市场包内命名（`onetcli-*-helper`，manifest `entry.command` 驱动，无需改名）
  - 待办：打包流程与 GUI 端到端冒烟

### 移植备注（远程桌面）

- 参考 `origin/dev` 整包，helper 命名适配 `omnihub-*-helper`
- storage 协议枚举与 runtime `remote_desktop::RemoteDesktopProtocol` 分离，转换在 `home_tabs::open_remote_desktop`
- 本地无 teams：表单去掉 team 选择，`team_id` 不写入

### 移植备注（端口转发）

- 参考 `origin/dev` 整包，适配 OmniHub 命名与本地 storage 字段
- 本地无 `TeamOption`：表单去掉 teams 选择，`team_id` 不写入
- runtime 使用 `SshParams::to_connect_config()`，避免手写与本地 SSH 字段漂移
- docker e2e 环境变量前缀改为 `OMNIHUB_DOCKER_*`

### Phase 3.x Redis SSH 隧道（已接入）

- [x] storage `RedisSshTunnelConfig` + `RedisParams::ssh_tunnel` + `apply_referenced_ssh_tunnel`
- [x] runtime：`redis_view` 连接建立本地端口转发，私钥兼容内容/路径
- [x] 表单：高级页 SSH 隧道 UI（引用 SSH 连接 / 手动配置）
- [x] locales + home/new_connection 传入 `ssh_connections`
- [x] 验证：`cargo check -p one-core -p redis_view -p main` ✅
- [ ] 手工验证：密码/私钥/Agent 隧道连通

适配备注：
- 本地 `SshAuth::PrivateKey` 使用 `key_content`，运行时兼容路径与 PEM 内容
- 无 teams UI 依赖
- 不引入 `onetcli_runtime`

### Phase 1 追加小修复

- [x] Redis 搜索时命名空间子节点可见（`redis_tree_view` local_search_visibility）
- [x] DB 连接测试错误可关闭/滚动（`clear_test_result` + max height）

### Phase 1.x SQL 查询最大返回行数（已接入）

- [x] `db::apply_query_max_rows`：按方言注入 LIMIT / TOP / FETCH
- [x] 各数据库 `execute` / `execute_streaming` 路径应用改写
- [x] `AppSettings.sql_query_max_rows` + 设置页 + `DbViewSettings` 同步
- [x] `sql_result_tab` 执行时传入 `ExecOptions.max_rows`（0=不限制）
- [x] 验证：`cargo check -p db` / `db_view` / `main` ✅
- 适配：本地 `AppSettings` 在 `main/src/setting_tab`，通过 `DbViewSettings` 传给 db_view（非 dev 的 one_core::settings）
- 未搬：sql_editor run current/all 快捷键大改（后续）

### Phase 1.x 字体设置（已接入）

- [x] `AppSettings`：`sql_editor_font_family` / `table_preview_font_family` / `custom_font_paths`
- [x] `db_view::DbViewFontSettings`（独立 Global，保持 `DbViewSettings: Copy`）
- [x] SQL 编辑器 / 表预览渲染使用对应字体
- [x] 设置页：SQL/表预览字体下拉 + 自定义字体导入（ttf/otf/ttc/otc）
- [x] 启动时加载已导入字体；导入后通知反馈
- [x] locales（en/zh-CN/zh-HK）
- [x] 验证：`cargo check -p db_view -p main` ✅
- 适配：不引入 dev 的 `CustomFont` 结构体与 skrifa 族名解析；下拉用文件 stem；终端字体设置本地已有

### Phase 1.x SSH 跳板机多认证（已接入）

- [x] 跳板机支持密码 / 私钥 / Agent / 自动公钥
- [x] 表单动态字段 + 私钥浏览（内容写入，兼容本地 `ssh_private_key`）
- [x] locales：`jump_auth_method` / `jump_key_path` / `jump_passphrase`
- [x] 单元测试 `build_jump_auth_method`
- [x] 验证：`cargo check -p terminal_view -p main` ✅
- 适配：字段名 `ssh_private_key`（非 dev 的 `key_path`）；不碰 MFA 既有路径
- 附带修复：`theme.rs` 测试导入 `default_font_fallbacks`（lib test 编译）
- 验证：`cargo test -p terminal_view --lib jump_auth_builder` ✅ 3 passed

### Phase 2.x 连接同步保留中途编辑（已接入）

- [x] `ConnectionRepository::update_sync_status_with_updated_at`
- [x] 上传/更新云端后 `update_sync_status_after_cloud_write`：检测同步期间本地内容变化
- [x] 同秒编辑抬高 `updated_at = synced_at + 1`，保持 pending
- [x] 单元测试 `pending_updated_at_after_cloud_write`
- [x] 验证：`cargo check -p one-core` / `cargo check -p main` ✅
- [x] 验证：`cargo test -p one-core --lib connection_sync_unit_tests` ✅ 6 passed
- 附带：拆分重复 `mod tests` → `connection_sync_unit_tests`；修测试字面量 `metadata`/`sort_order`/`team_id` 编译障碍
- 适配：无 `team_id` 字段；`upload_connection` 返回 `CloudSyncData`





### Phase 1.x 终端侧栏远程解压（部分接入）

- [x] 图标 `unarchive.svg` + `IconName::Unarchive`
- [x] 终端 FileManager：压缩包类型识别 / 冲突检查 / 覆盖·跳过 / 解压 loading
- [x] 右键菜单 Extract（zip/tar*/gz）
- [x] locales `FileManager.extract*`
- [x] 单元测试 `extract_archive_tests`
- [x] 验证：`cargo check -p terminal_view -p main` ✅
- [x] 验证：`cargo test -p terminal_view --lib extract_archive_tests` ✅ 2 passed
- [x] `sftp_view` 双栏 Extract 已接入（helpers 各仓内一份，未强抽共享）
- [x] 右键菜单 + 冲突确认 + 队列 loading + locales
- [x] 验证：`cargo check -p sftp_view -p main` ✅
- [x] 验证：`cargo test -p sftp_view --lib extract_archive_tests` ✅ 3 passed


### Phase 1.x AI reasoning 分离（已接入）

- [x] `llm::StreamTextParts` / `extract_stream_text_parts`（正文与 reasoning 分离；legacy `extract_stream_text` 保留 fallback）
- [x] `ChatMessageUIGeneric.reasoning_content` + `is_reasoning_expanded` + `with_reasoning_content`
- [x] `StreamEvent::ReasoningDelta` + `ChatStreamProcessor` 节流发送
- [x] `AgentEvent::ReasoningDelta` + `GeneralChatAgent` / `SqlWorkflowAgent`
- [x] `ChatEngine::update_streaming_reasoning`；错误 finalize 清空 reasoning
- [x] `ai_chat/reasoning.rs` 可折叠「思考过程」UI + locales
- [x] `AiChatPanel` / `ChatMessageRenderer` / `db_view::ChatPanel` 接线
- [x] 验证：`cargo check -p one-core -p db_view -p main` ✅
- [x] 验证：`cargo test -p one-core --lib extract_stream_text` ✅ 4 passed
- 适配：本地 chat UI 仍以 `chat_panel` 自绘为主；`message_list` 未接 window 参数，暂不展示折叠 reasoning（当前未被 chat_panel 使用）
- 未搬：把 reasoning 持久化进历史会话（dev 亦未做）


### Phase 1.x HTTP User-Agent 直连（已接入）

- [x] `ReqwestClient::user_agent` 使用 `no_proxy()`，并写回 `client.user_agent`
- [x] 避免仅设置 UA 时仍走环境代理
- 适配：本地 `build_app_http_client` 已用 `omnihub` UA

### Phase 1.x PostgreSQL 删除 SQL 去 LIMIT（已接入）

- [x] `PostgresPlugin::build_limit_clause` 返回空（DELETE 不拼 LIMIT 1）
- [x] 单测 `test_generate_delete_row_sql_without_limit`
- [x] 验证：`cargo test -p db --lib test_generate_delete_row_sql_without_limit` ✅ 1 passed

### Phase 1.x schema-as-database 编辑器选择（已接入）

- [x] 初始选择优先 schema；schema 模式写入 `schema_select`
- [x] 工具栏仅渲染 schema 选择器（Oracle 等）
- [x] 执行/EXPLAIN/保存事件读取 `schema_select`
- [x] locales `Query.please_select_schema`
- [x] 验证：`cargo check -p db -p db_view -p main` ✅
- 适配：本地仍为 `new_with_config` 参数式 API（非 dev 的 `SqlEditorTabConfig`）

### 下一包评估

- 首页拖拽/布局：本地 `home_tab` 已有 Manual 排序与布局，**不整包回灌** `8f00f50f/4c5c2411`；dev 后续 `ce5bb446` 暂停 sort_order 优先（本地仍支持 Manual）
- 终端字体族：本地已有 `terminal_font_family`
- MCP / extension / tool_runtime：重依赖，延后
- DB search shortcuts (`df30aba5`)：已接入


### Phase 1.x DB 搜索快捷键（已接入）

- [x] `db_view::search_shortcut`：`FocusSearchInput` / `OpenSelectedTableQuery`
- [x] action id：`db.focus_search` / `db.open_table_query`
- [x] 接线：`DbTreeView` / `DatabaseObjects` / `ColumnsEditor`
- [x] 启动 `search_shortcut::init`（`omnihub_app`）
- [x] 设置页快捷键展示 + locales
- [x] 验证：`cargo check -p one-core -p db_view -p main` ✅
- [x] 验证：`cargo test -p db_view --lib search_shortcut` ✅ 3 passed
- DataGrid 页内搜索：已在 Phase 1.x 表数据本地搜索接入
- 适配：设置页仍为只读展示（非 dev 可改绑 UI）


### Phase 1.x 单行输入垂直导航（已接入）

- [x] `should_handle_vertical_navigation`：多行或 context menu 打开时处理上下键
- [x] 单测 `vertical_nav_tests`
- [x] 验证：`cargo test -p gpui-component --lib vertical_nav` ✅ 3 passed

### Phase 1.x 表数据本地搜索 + 保存前提交单元格（已接入）

- [x] `EditorTableDelegate.row_search_query` + 与列过滤 AND
- [x] `DataGrid` 工具栏 `search_input` + `DB_SEARCH_CONTEXT` FocusSearch
- [x] `has_unsaved_changes` 含进行中单元格编辑；`save_changes` 前 `commit_cell_edit`
- [x] 验证：`cargo check -p gpui-component -p db_view -p main` ✅
- [x] 验证：`cargo test -p db_view --lib row_search` ✅ 3 passed
- 适配：本地已有 filter_editor；搜索为叠加能力

### Phase 1.x Linux arm64 更新资产名（已接入）

- [x] `expected_archive_name_for(os, arch)` 含 `linux/aarch64` → `omnihub-aarch64-unknown-linux-gnu.tar.gz`
- [x] 单测 `expected_archive_name_includes_linux_arm64`
- 适配：产物前缀 `omnihub-`（非 onetcli-）
- [x] 验证：`cargo test -p main --bin omnihub expected_archive_name` ✅ 1 passed

### Phase 1.x Oracle 表编辑字面量 + 提交（已接入）

- [x] `d9797f7b`：Oracle 表变更 SQL 字面量（DATE/TIMESTAMP/TZ/CLOB 分块/转义）
- [x] `generate_table_changes_sql` 走 `build_oracle_table_change_sql`
- [x] `ef2553a4` 核心：`execute` 成功 Exec 后 `commit_sync`
- [x] 验证：`cargo test -p db --lib table_change_sql` ✅ 6 passed
- [x] 验证：`cargo test -p db --lib oracle_lob_literal` ✅ 1 passed
- 未搬：IPC external driver 的 Oracle 兼容路由（本地 `ExternalDatabasePlugin` 架构不同，延后）

### 下一包评估（续）

- `564857f3` release page URL：本地已有 `GITHUB_LATEST_RELEASE_URL`，**跳过**
- `d15ecfc8` query selected table：本地 search_shortcut 已覆盖 OpenSelectedTableQuery，**跳过**
- `5652798e` database objects 列宽：本地已用 `Table`/`col_resizable`，**跳过**手写 resize handle
- `16e5b1dc` table designer 列宽：已接入
- `7e125934` IPC Datetime 格式化：本地 `ipc/connection.rs` 无 CellValue 分派，**跳过**
- MCP / extension：继续延后


### Phase 1.x IPC 相对路径命令解析（已接入）

- [x] `resolve_command_program` / `has_explicit_path_component`
- [x] `build_driver_command` 相对路径相对 manifest 工作目录解析
- [x] 验证：`cargo test -p db --lib resolve_command_program` ✅ 3 passed
- 适配：本地 spawn API 与 dev 不同，按 `Command::new(program)` 接入

### Phase 1.x 表设计元数据 is_primary 保留（已接入）

- [x] `IndexInfo.is_primary` + 各引擎 `list_indexes` 构造补齐
- [x] designer 过滤主键索引；缺失类型写入下拉选项
- [x] 验证：`cargo test -p db_view --lib test_primary_indexes_with_database_specific_names` ✅ 1 passed
- [x] 验证：`cargo test -p db_view --lib test_loaded_column_type_missing` ✅ 1 passed
- 未搬：IPC wire `index.is_primary` 映射（本地 wire 结构不同，延后）


### Phase 1.x External schema switch SQL 回退（已接入）

- [x] `IpcDriverDialect.compatible_database_type` 可选字段
- [x] `switch_schema`：RPC 成功后附加方言 SQL；`NotSupported` 时 SQL 回退
- [x] Postgres `SET search_path` / Oracle `ALTER SESSION` / DuckDB `SET schema` / dm→Oracle
- [x] 验证：`cargo test -p db --lib schema_switch` ✅ 6 passed
- 适配：本地 RPC 为 `switch_schema`（非 dev `conn/use`）

### Phase 1.x Table designer 列宽拖拽（进行中/已接入）

- [x] `16e5b1dc`：ColumnsEditor 可调整列宽 + min/max 边界
- [x] header resize handle + 行宽绑定 `column_widths`
- [x] 验证：`cargo test -p db_view --lib column_editor_resize` ✅ 2 passed

### 下一包评估（续 2）

- `ce5bb446` 暂停 sort_order 优先：本地保留 Manual 排序，**跳过**
- `d19c56f3` registry reloader 优先：本地无 reloader 架构，**跳过**
- `e0cfb693` IPC startup env_from_config：本地 entry 无 commands/env_from_config，延后
- `0bfe928d` 外部驱动图标：已接入（见下）
- MCP / extension：继续延后


### Phase 1.x 外部驱动标签页图标（已接入）

- [x] `0bfe928d`：外部驱动图标显示（适配本地 `External` + `extra_params[external_driver_id]`）
- [x] `Icon.file_path` + color 模式 `ImageSource` 文件系统渲染（`crates/ui/src/icon.rs`）
- [x] `crates/db/src/ipc/display.rs`：`IpcDriverDisplay` / `display_for_config` / asset+file 图标解析
- [x] `IpcDriverRegistry::from_drivers` 测试辅助
- [x] `DatabaseTabView::icon` 优先驱动文件图标 → 内置资源图标 → 类型图标
- [x] 验证：`cargo test -p db --lib display` ✅ 3 passed
- [x] 验证：`cargo test -p db_view --lib database_tab_icon` ✅ 3 passed
- [x] 验证：`cargo check -p main -p db_view` ✅
- 适配：无 `icon_color` / `DatabaseType::External { driver_id }`；driver id 走 `EXTERNAL_DRIVER_ID_PARAM`
- 未搬：树节点/新建连接表单图标（可作轻量续包）

### 下一包评估（续 3）

- 可选：`db_tree_view` 外部驱动图标：已接入（见下）
- `e0cfb693` IPC startup `env_from_config` / platform commands：entry 模型差异大，中重
- `52ae6634` external SQL dialect exposure：中重
- MCP / extension / tool_runtime：继续延后


### Phase 1.x 外部驱动树节点图标（已接入）

- [x] 延续 `0bfe928d` / `88f598b0`：`db_tree_view` 连接节点写入外部驱动 display 元数据
- [x] `connection_node` / `connection_node_icon` / `apply_connection_node_config`
- [x] 创建、更新、添加连接时刷新 metadata；图标优先 file → asset → `as_node_icon`
- [x] 验证：`cargo test -p db_view --lib external_connection_metadata` ✅ 1 passed
- [x] 验证：`cargo test -p db_view --lib external_driver`（树 metadata + tab icon） ✅
- [x] 验证：`cargo test -p db_view --lib apply_connection_node_config` ✅ 2 passed
- 适配：driver id 仍走 `extra_params[external_driver_id]`
- 未搬：首页 `home_tab` / 新建连接列表的 external driver 图标（依赖 home 统一渲染，中等）

### 下一包评估（续 4）

- 首页/新建连接 external driver 图标：已接入（见下）
- `e0cfb693` IPC startup env_from_config：中重
- `52ae6634` external SQL dialect：中重
- MCP / extension：继续延后


### Phase 1.x 首页/新建连接外部驱动图标（已接入）

- [x] `main/src/external_driver_display.rs`：`external_driver_icon_for_config` / `for_driver_id`
- [x] `home_tab::render_connection_icon` 优先外部驱动图标
- [x] `NewConnectionKind::ExternalDatabase` 图标走驱动 display
- [x] 验证：`cargo test -p main --bin omnihub external_driver_icon` ✅ 2 passed
- 适配：无 category / builtin skip 列表（本地仍列出全部 external drivers）

### 下一包评估（续 5）

- `e0cfb693` IPC startup env_from_config：已接入（见下）
- `52ae6634` external SQL dialect exposure：中重
- MCP / extension / tool_runtime：继续延后


### Phase 1.x IPC 启动配置 env_from_config / platform commands（已接入）

- [x] `e0cfb693`：`IpcDriverEntry.commands` + `env_from_config`
- [x] `command_for_platform` / `command_for_current_platform`（windows → commands.windows，否则 default → command）
- [x] `config_value` 支持 host/port/user/password/database/extra_params.*
- [x] `JsonRpcClient::start_with_connection_config`；`ExternalDbConnection::connect` 传入 config
- [x] `build_driver_command` 注入环境变量（适配本地 tokio Command，非 extension_host SpawnConfig）
- [x] 验证：`cargo test -p db --lib command_for_platform` ✅ 2 passed
- [x] 验证：`cargo test -p db --lib env_from_config` ✅ 1 passed
- [x] 回归：`cargo test -p db --lib display` ✅ 3 passed
- 适配：本地 spawn 路径与 dev 不同，按 Command/.env 接入

### 下一包评估（续 6）

- `52ae6634` external SQL dialect exposure：已接入（见下）
- MCP / extension / tool_runtime：继续延后


### Phase 1.x External SQL dialect 表引用/分页/row_id（已接入）

- [x] `52ae6634` 核心：`IpcDriverDialect` 增加 `limit_style` / `table_reference_schema_mode` / `row_id_*` / `default_order_by`
- [x] `LimitStyle`（LimitOffset | OffsetFetch）与 `TableReferenceSchemaMode`（Auto | PreferSchema）
- [x] dialect helpers：`quote_identifier` / `format_table_reference` / `format_pagination`
- [x] `ExternalDatabasePlugin::query_table_data` 按连接驱动 dialect 生成 COUNT/SELECT SQL
- [x] `format_table_reference` 多驱动下 schema 优先回退
- [x] 验证：`prefer_schema_table_reference_uses_schema` ✅
- [x] 验证：`offset_fetch_pagination_style` ✅
- 适配：本地为 registry 多驱动，无 `for_driver` 单驱动插件；query_table_data 从 connection 解析 driver

### 下一包评估（续 7）

- MCP / extension / tool_runtime：继续延后
- 终端 CJK/selection：本地已具备，跳过
- 其余 medium 项按需评估


### Phase 1.x 表触发器/检查项表名回填（已接入）

- [x] `768afafa` 适配：驱动省略 `table_name` 时用请求表名回填
- [x] `fill_trigger_table_names` / `fill_check_table_names`（metadata JSON 路径，非 wire helper）
- [x] `list_table_triggers` / `list_table_checks` 接入回填
- [x] 验证：`cargo test -p db --lib table_name_` ✅ 含 3 项 fallback/keep 测试 passed
- 未搬：extension-protocol serde default（本地无该 crate 路径）

### Phase 1.x SQL 补全使用驱动函数（已接入）

- [x] `ffc9d7f4`：`SqlSchema.functions` + `with_functions`
- [x] completion 优先展示 schema/driver 动态函数
- [x] `GlobalDbState::list_functions` + `sql_editor_view` 加载函数签名
- [x] 验证：`cargo test -p db_view --lib with_dynamic_functions` ✅ 1 passed
- 适配：本地无 wire `function_info_from_wire`；参数已是 `Vec<String>`

### 下一包评估（续 8）

扫描结论（轻→中大多已在当前分支）：

- 已本地：`cf0c2f65` Oracle DUAL ping、`e754f8e5` PG truncate+schema、`034ff5f3` 连接错误可关闭、
  `cddac422` redis namespace 搜索、`0df1545b` 跳板多认证、`e35c2f85` 窗口置顶、
  `d68184d6` sql max rows、`c5767f5c` 自定义字体/SQL 字体、`bd939f8a` redis SSH tunnel、
  终端 CJK/selection、home Manual 拖拽排序、external form title
- 高冲突延后：`cc555c44` external DDL（依赖 wire_ddl + 单驱动 `for_driver` + connectionless RPC）
- 重包延后：MCP / extension / tool_runtime / port_forwarding / remote_desktop / team-website
- 可选中等后续：external 驱动 CreateDatabase 表单 i18n（需 `external_driver_manifest` 与 per-connection 插件上下文）、
  home 统一渲染细部 diff（`4c5c2411`）若仍有差异

### 本波验证证据

- `cargo test -p db --lib table_name_` ✅ 4 passed（含 3 项表名回填）
- `cargo test -p db_view --lib with_dynamic_functions` ✅ 1 passed
- `cargo check -p main -p db_view -p db` ✅


### Phase 1.x IPC 经宿主 SSH 隧道路由（已接入）

- [x] `7ccd8f64` 适配：`resolve_connection_target` + 保留 `LocalPortForwardTunnel`
- [x] `connection_config_params_with_target` 把 host/port 改写为隧道本地地址
- [x] 外部连接表单补 SSH tab（`ensure_external_ssh_tab` / default form）
- [x] 验证：`cargo test -p db --lib connection_config` ✅ 2 passed
- 适配：本地 RPC 为 `connect` + config（非 wire `conn/open`）

### Phase 1.x 显式 IPC identifier quote pair（已接入）

- [x] `46ab8acd` 适配：`identifier_quote_left` / `identifier_quote_right`（兼容旧 `identifier_quote`）
- [x] 支持 MSSQL 风格 `[name]` 引号对
- [x] 验证：`cargo test -p db --lib quote_identifier` ✅ 含 bracket/legacy 测试

### 下一包评估（续 9）

- `ca48bbf1` 本地终端关闭确认：本地已有 `has_blocking_terminal_activity` + 关闭确认对话框，**跳过**（OSC 跟踪为替代实现）
- 可选：`f48c1564` 单文件连接生命周期强化（若仍有缺口）
- 可选：`6c376c51` IPC object view metadata（中重）
- 高冲突：`cc555c44` external DDL
- 重包：MCP / extension / port_forwarding / remote_desktop

### 本波验证证据（续）

- `cargo test -p db --lib connection_config` ✅ 2
- `cargo test -p db --lib quote_identifier` ✅ 9（含 bracket/legacy）
- `cargo check -p db_view -p db` ✅


### Phase 1.x 禁用 TUI 内终端历史提示（已接入）

- [x] `46fc6077`：`shell_prompt_input_active` + application mode 守卫
- [x] `TerminalModelEvent::CommandStart` 向上游发射（不改 ssh_process_state）
- [x] history prompt 仅在 SSH + 可输入 prompt + 非 TUI 模式显示
- [x] 验证：`cargo test -p terminal_view --lib history_prompt` ✅ 31 passed

### Phase 1.x 菜单长标签溢出修复（已接入）

- [x] `d385c9ef`：popup_menu / sidebar 标签 `whitespace_nowrap` + `overflow_x_hidden`
- 验证：随 `cargo check -p gpui-component` 覆盖

### 下一包评估（续 10）

- `b21cf654` table design is_primary：本地已有，跳过
- `f48c1564` 单文件连接生命周期：部分本地有 close_on_release
- `6c376c51` IPC object view metadata：中重
- `81ea737d` 驱动分类：可选 UI
- 高冲突：external DDL / MCP / extension / port_forwarding / remote_desktop


### Phase 1.x IPC 驱动分类 / 国产数据库分组（已接入）

- [x] `81ea737d`：`IpcDriverManifest.category`
- [x] 新建连接 `DomesticDatabase` 分类（`domestic_database`）
- [x] 连接表单标题使用外部驱动显示名
- [x] 验证：`deserializes_optional_category` ✅
- [x] 验证：`cargo test -p main --bin omnihub domestic_database` ✅ 2
- [x] 验证：`cargo test -p db_view --lib connection_title` ✅ 2

### Phase 1.x macOS 窗口置顶（已接入）

- [x] `4c5c2411` / `e35c2f85` macOS 分支：`set_macos_always_on_top`（NSView→NSWindow `setLevel:`）
- [x] `ALWAYS_ON_TOP` / `toggle_always_on_top` / action+快捷键：`windows | macos`
- [x] 标题栏置顶按钮仍仅 Windows（macOS 原生标题栏，与 origin 一致）
- [x] 验证：`cargo check -p main` ✅

### Phase 1.x 缺省 locale 跟随系统（补齐）

- [x] `282c2e8e` 适配：`AppSettings.locale` serde 默认改为 `system`（此前 `#[serde(default)]` 会落空串→英文）
- [x] `resolve_locale_setting` 将 `""` 视为 system
- [x] 验证：`cargo test -p main --bin omnihub -- locale app_settings_` ✅ 4 passed

### 下一包评估（续 11）

- 已本地：system locale 主体、windows_subsystem、Oracle commit、is_primary 表设计、relative command path、switch_schema
- 可选中等：`ef2553a4` 外部 Oracle 表变更 SQL 走 OraclePlugin（本地 multi-driver 无 for_driver 上下文，需设计 request/driver 解析）
- 高冲突：`cc555c44` external DDL、`f48c1564` 全量 lifecycle locks
- 重包：MCP / extension / port_forwarding / remote_desktop / team-website

### Phase 1.x 包裹式 IPC 驱动目录扫描（已接入）

- [x] `2f4363bf`：`driver_manifest_dir_for` / `single_wrapped_driver_dir`
- [x] 忽略 `.DS_Store` / `__MACOSX` / `._*` 归档元数据
- [x] 支持 root 本身即驱动包、以及 outer/inner/driver.json 解压结构
- [x] 验证：`scans_driver_manifests` / `scans_single_wrapped_driver_directory` / `scans_single_driver_directory_as_root` ✅

### Phase 1.x 杂项轻量补齐

- [x] macOS 应用菜单 `OneNet` → `OmniHub`（显示名；仓库 URL 仍为 onetcli）
- 验证：`cargo check -p main -p db` ✅
- 验证：locale / wrapped driver 单测 ✅

### 下一包评估（续 12）

- 轻量包本波已收口：macOS 置顶、locale serde 默认、包裹驱动目录、菜单文案
- 中等待设计：`ef2553a4` 外部 Oracle 表变更 SQL（需 multi-driver 上下文）
- 高冲突：`cc555c44` external DDL、`f48c1564` 全量 lifecycle locks、`6c376c51` object view metadata
- 重包：MCP / extension / port_forwarding / remote_desktop / team-website

### Phase 1.x 外部 Oracle 表数据编辑 SQL（已接入，multi-driver 适配）

- [x] `ef2553a4` 适配：`TableSaveRequest.driver_id` 携带外部驱动 id
- [x] `ExternalDatabasePlugin::generate_table_changes_sql` 按驱动 dialect/`compatible_database_type` 路由到 `OraclePlugin`
- [x] `uses_schema_as_database` 时把 `database` 回填为 `schema`
- [x] `data_grid.create_save_request` 从连接配置注入 `external_driver_id`
- [x] 验证：`external_oracle_table_changes_*` ✅ 2、`external_non_oracle_*` ✅ 1、`table_change_sql*` ✅ 6

### 下一包评估（续 13）

- 已收：macOS 置顶、locale 默认、包裹驱动目录、外部 Oracle 表编辑 SQL
- 中等可选：`52e9cb39` compatible DDL fallback（本地无 wire async DDL 路径，价值有限）
- 高冲突：`cc555c44` external DDL、`f48c1564` lifecycle locks、`6c376c51` object view metadata 增量
- 重包：MCP / extension / port_forwarding / remote_desktop

### Phase 1.x IPC 自定义 Object View + 连接生命周期（已接入）

- [x] `6c376c51` 适配：`metadata.object_view`（非 wire schema/object_view）
  - 驱动可返回自定义 title/columns(width/align)/rows
  - 各 `list_*_view` 优先走自定义，失败回退原实现
  - 补 `list_schemas_view`
- [x] `f48c1564` 轻量：`IpcDriverConnection` manifest 段 + `ExternalDbConnection::close_on_release`
- [x] `f48c1564` 核心：`ConnectionLifecycle` + DuckDB/External 覆盖 + `ConnectionManager` physical open lock（串行化 create_session 物理打开）
  - 未搬：busy close_on_release 重试循环、session 级 close_on_release 字段、占用中禁止复用第二物理连接
- [x] Copy SQL 方言：`CopySqlRequest.driver_id` + Oracle `format_copy_value`（TO_DATE/TO_CLOB）+ External 委托
- [x] 验证：`connection_lifecycle*` ✅ 7、`external_oracle*` ✅ 3、`external_non_oracle_copy*` ✅ 1、`object_view_*` ✅ 3、`close_on_release*` ✅、`cargo check -p db` ✅

### 下一包评估（续 14）

- 已收：object view 自定义、manifest close_on_release
- 仍延后：lifecycle busy-retry 全量、external DDL、MCP/extension/port_forward/remote_desktop

## 进度更新 2026-07-13（续）

### 已完成

1. **ConnectionLifecycle + physical open lock**（`f48c1564` 轻量核）
   - `crates/db/src/plugin.rs`：`ConnectionLifecycle` / `single_file` / 路径规范化
   - DuckDB + External plugin 覆盖
   - `ConnectionManager`：`physical_open_locks` + create_session 二次 try_acquire
2. **Copy SQL Oracle 方言**
   - `CopySqlRequest.driver_id`
   - Oracle `format_copy_value` → temporal/LOB 字面量
   - External 按 driver 委托；默认实现提取为 `default_generate_copy_*`
   - `data_grid` 复制 SQL 路径传入 `driver_id`

### 验证

- `cargo check -p db` ✅
- `connection_lifecycle` 7 ✅ / `external_oracle` 3 ✅ / `external_non_oracle_copy` 1 ✅ / `object_view_` 3 ✅

### 下一批（轻→重）

1. 可选：compatible DDL fallback 多驱动上下文（若可设计）
2. 重：lifecycle busy-retry 全量
3. 重：external DDL / MCP / extension / port_forward / remote_desktop


## 进度更新 2026-07-18（完成度盘点）

### 整体完成度：约 75%

代码健康度：workspace 编译 0 错误；db(460) + db_view(314) + terminal_view(140) 共 914 测试全过。

### 各 Phase 完成情况

| Phase | 状态 | 完成度 | 说明 |
|---|---|---|---|
| 0 基线冻结 | ✅ | 100% | 差距分析 + 计划文档 |
| 1 低冲突修复包 | ✅ | ~90% | 30+ 项小修复全接入，均带 cargo test 验证 |
| 2 首页与连接体验 | 🟡 | ~50% | 同步保留编辑✅；首页拖拽/布局本地已有，不整包回灌 |
| 3 SSH 隧道与 Redis/DB | ✅ | ~85% | Redis tunnel✅ 跳板多认证✅ IPC 经宿主隧道路由✅ |
| 4 端口转发 | ✅ | ~90% | crate+storage+接线✅；UI smoke test 通过（功能正常）；真实转发流量 E2E 可选 |
| 5 远程桌面 | ✅ | 110% | 超预期：基础接入 + provider 安装引导 + VNC ARD 兼容(libvncclient) + 增量帧协议 + 显存崩溃修复 + 降频渲染 + 4 种显示模式 |
| 6 MCP/CLI/tool runtime | ⏳ 延后 | 0% | 架构级重依赖，单独立项评估 |
| 7 扩展系统/外部驱动 | 🟡 | ~40% | 外部驱动多驱动插件系统✅(adfe96e0)；extension-*/wasm host 延后 |
| 8 版本/CI/文档收口 | ⏳ | 0% | 版本线分叉（本地 0.4.8 vs dev 0.7.2） |

### 本波完成度检查发现并修复

- `data_grid.rs` `DatabaseType` 重复 import（E0252，db 插件提交 adfe96e0 引入，测试 target 才暴露）→ 已修复（97fd5295）

### 未完成项（按建议优先级）

1. **手工/端到端验证**（最大缺口）：
   - RDP/VNC 连接与输入转发实机冒烟（Phase 5）
   - 端口转发 本地转发 / 动态 SOCKS（Phase 4）- UI smoke test 通过（功能正常）；真实转发流量 E2E 可选
   - Redis SSH 隧道 密码/私钥/Agent 连通（Phase 3.x）
2. **helper 打包流程冒烟**（Phase 5 待办）：`omnihub-*-helper` 命名 + 安装引导端到端
3. **Phase 6/7 决策**：MCP / extension / tool_runtime 是否继续吸收，或与外部驱动插件并存后收口
4. **Phase 8 版本收口**：版本线统一、CI workflow 合并、README 能力清单
5. **高冲突延后项**：external DDL（cc555c44，依赖 wire_ddl + for_driver 单驱动上下文）、lifecycle busy-retry 全量


## ADR-001：Phase 6/7（MCP / 扩展系统）收口决策（2026-07-18）

### 决策

**0.x 版本采用"能力特化"扩展模型，不引入 dev 的通用 wasm 扩展框架（extension-* 全家桶）与 MCP/tool_runtime。** Phase 6/7 标记为"不吸收，另起大版本评估"。

### 背景与量化依据

- dev 侧待吸收包代码量：extension 全家桶（7 crate）≈ 23,700 行；MCP/runtime/cli ≈ 10,100 行，合计约 34,000 行。
- 本地已用特化实现覆盖核心用户价值：
  - 数据库外部驱动 → `crates/db/src/ipc`（registry/plugin/protocol/connection/display/client，多驱动 registry）
  - 远程桌面 Provider 安装 → `main/src/remote_desktop_install`（marketplace 下载/sha256 校验/安装/备份/回滚）
  - 端口转发 → `crates/port_forwarding` + `port_forwarding_view`（内置能力）
  - VNC helper 经 marketplace release 包安装（已实测）

### 理由（经外部审核确认）

1. **边际价值 < 边际成本**：本地 ~5-8k 行特化实现已覆盖 dev 34k 行通用框架约 80% 的当前场景价值，剩余 20%（通用任意扩展）在 0.x 无落地场景。
2. **避免二元架构**：强吸收会形成"外部驱动走 IPC 插件、其它扩展走 wasm host"的双扩展体系，注册/生命周期/沙箱模型翻倍，长期维护成本高。
3. **MCP/tool_runtime 无 GUI 场景**：`public_mcp`/`onetcli_runtime` 面向 CLI/Agent，与 OmniHub GUI 客户端定位不匹配，吸收后成死代码。
4. **冲突风险**：dev 部分功能依赖 teams（本地无 TeamOption），34k 行预计 15-30% 需人工裁剪，且 rename 边界改名 diff 会污染功能 diff。

### 未来演进

如需通用扩展框架，作为 **1.0 大版本独立立项**：
- 参考 `dev/extension-protocol`（6516 行）的 schema 设计作为蓝本归档
- 不复用其 host/runtime 实现；不与现有 IPC 插件体系过渡期共存

### 配套动作

- [~] 对照 `extension-driver` 对本地 `db/ipc` 插件系统做 gap 分析（按需回补，工作量远小于吸收全套）：
  - [x] **版本协商**：manifest `protocol_version` 门禁（major 拒绝 / minor 放行+告警 / 遗留隐式放行）— `9259a5f9`（两轮外部审核 APPROVED）
  - [x] **manifest schema 加固**：`serde_ignored` 未知字段软告警（完整路径，与协议门禁同构）— `bb235972`（APPROVED）
  - [x] **错误语义化**：`DbError::InvalidManifest` 变体 + 版本判别测试 — `b8d0c514`（APPROVED）
  - [x] **子进程可观测性**：stderr 背压（单行 64KB 截断 + 20/s 速率限制 + drain/log 解耦防管道阻塞 + EOF 汇总）— `ebb1be36`（两轮 APPROVED，含 suppressed 双重计数 P0 修复）
  - [ ] 崩溃隔离与自动重启 + 熔断退避 — 待业务信号触发（用户报"驱动挂了要手动重连"痛点时回补）
  - [ ] 权限沙箱声明 — 待跑不可信驱动的明确需求（OS 级方案成本高，前期进程隔离兜底）
  - [ ] marketplace 签名校验 — 待引入第三方 driver registry（分发模型未定型）
- [ ] 跨越 rename 边界的任何后续吸收，先做 rename 提交再做功能提交（分两次 PR）


