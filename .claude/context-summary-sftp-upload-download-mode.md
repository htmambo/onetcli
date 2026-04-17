## 项目上下文摘要（SFTP 上传下载模式收口）
生成时间：2026-03-28 03:32:30 +0800

### 1. 相似实现分析
- **实现1**: [`crates/terminal_view/src/sidebar/file_manager_panel.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/sidebar/file_manager_panel.rs)
  - 模式：侧边栏文件管理器通过系统路径选择器上传，通过目录选择器决定下载目标
  - 可复用：面板模式下继续保留 `select_and_upload_files`、`select_and_upload_folder`、`download_item`
  - 需注意：面板只展示远程侧列表，本地路径不在视图内维护

- **实现2**: [`crates/sftp_view/src/lib.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/lib.rs)
  - 模式：独立 SFTP 页面双栏分离，本地侧基于当前选择上传，远程侧基于当前选择下载
  - 可复用：`upload_selected`、`download_selected`、工具栏按钮禁用状态
  - 需注意：动作对象取自 `selected_items()`，若右键命中项不在当前选区，容易误操作旧选区

- **实现3**: [`crates/sftp_view/src/file_list_panel.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/file_list_panel.rs)
  - 模式：文件项菜单和面板空白区菜单统一回发 `FileListPanelEvent`
  - 可复用：`build_file_context_menu`、`build_panel_context_menu`、列表行 `.context_menu(...)`
  - 需注意：当前只有左键会同步选中项，右键不会主动更新选区

- **实现4**: [`crates/sftp_view/src/context_menu_handler.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/context_menu_handler.rs)
  - 模式：本地列表与远程列表的菜单事件在 `handle_local_context_menu_event` / `handle_remote_context_menu_event` 分流
  - 可复用：本地侧上传直接走 `upload_selected`，远程侧下载直接走 `download_selected`
  - 需注意：远程侧 `UploadFile` / `UploadFolder` 仍走系统选择器，和双栏模型不一致

### 2. 项目约定
- **命名约定**: Rust 函数与变量采用 `snake_case`，事件枚举使用 `CamelCase`
- **文件组织**: 文件列表交互保持在 `file_list_panel.rs`，SFTP 业务动作保持在 `lib.rs` 与 `context_menu_handler.rs`
- **导入顺序**: 标准库、第三方库、项目内模块分组导入，沿用现有格式
- **代码风格**: 小步修改现有 builder 链和事件分发，避免新建额外状态机

### 3. 可复用组件清单
- [`crates/sftp_view/src/lib.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/lib.rs): `upload_selected`、`download_selected`
- [`crates/sftp_view/src/context_menu_handler.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/context_menu_handler.rs): 本地/远程菜单事件分流
- [`crates/sftp_view/src/file_list_panel.rs`](/usr/htdocs/onetcli/crates/sftp_view/src/file_list_panel.rs): 文件项和空白区右键菜单构建
- [`crates/terminal_view/src/sidebar/file_manager_panel.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/sidebar/file_manager_panel.rs): 面板模式上传下载基线行为

### 4. 测试策略
- `cargo fmt --all -- crates/sftp_view/src/file_list_panel.rs crates/sftp_view/src/context_menu_handler.rs crates/sftp_view/src/lib.rs`
- `cargo check -p sftp_view`
- `cargo test -p sftp_view --lib --no-run`
- GUI 手工关注：
  - 独立页面本地侧右键上传作用于当前选择
  - 独立页面远程侧右键不再出现系统选择器上传入口
  - 右键未选中项时，下载/删除/重命名目标与命中项一致

### 5. 依赖和集成点
- **外部依赖**: `gpui`、`gpui_component`、`rust_i18n`
- **内部依赖**: `FileListPanelEvent`、`SftpView` 的本地/远程面板实体、`selected_items()`
- **集成方式**: 列表行右键先同步选区，再由上下文菜单事件触发业务动作
- **配置来源**: 菜单文案来自 `locales` 中现有 `Common.upload`、`Common.download`、`File.*`

### 6. 技术选型理由
- **为什么这样设计**: 保持“面板模式依赖系统选择器、独立页面依赖双栏当前路径”的职责分离，不混用两套上传模型
- **优势**: 交互一致，改动面集中在 `sftp_view`，不会影响终端侧边栏文件管理器
- **劣势和风险**: 右键同步选区会改变现有多选上下文菜单行为，需要确认是否符合桌面端常见预期

### 7. 关键风险点
- **命中与选区一致性**: 若右键同步逻辑处理不当，可能破坏多选场景
- **菜单可发现性**: 删除远程侧上传入口后，需要保留本地侧足够清晰的上传入口
- **验证范围**: 当前主要依赖编译验证，GUI 细节仍需桌面点测确认
- **安全考虑**: 本任务不涉及认证、权限或协议实现变更
