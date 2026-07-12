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
- [ ] 辅助进程打包/helper 命名路径回归（omnihub-*-helper）

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
- `16e5b1dc` table designer 列宽：与本地 designer 冲突风险高，稍后
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

