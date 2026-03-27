## 项目上下文摘要（SFTP 右键菜单补齐）
生成时间：2026-03-27 13:51:22 +0800

### 1. 相似实现分析
- **实现1**: crates/sftp_view/src/file_list_panel.rs:579
  - 模式：文件项行内通过 `.context_menu(...)` 构建菜单，菜单事件统一回发 `FileListPanelEvent`
  - 可复用：`build_file_context_menu`、`FileListPanelEvent`
  - 需注意：`..` 行只有双击逻辑，没有右键菜单

- **实现2**: crates/terminal_view/src/sidebar/file_manager_panel.rs:2285
  - 模式：侧边栏文件管理器在真实文件项行上绑定 `.context_menu(...)`，点击动作直接调用视图方法
  - 可复用：`build_context_menu`、`go_parent`、`refresh_dir`、上传与复制路径逻辑
  - 需注意：外层拖拽区域未绑定菜单，空白区右键无响应

- **实现3**: crates/redis_view/src/redis_tree_view.rs:1856
  - 模式：节点行级别右键菜单，菜单内容按节点类型动态构建
  - 可复用：按上下文区分菜单项的组织方式
  - 需注意：右键菜单绑定在具体可命中行上，不会自动覆盖空白区域

- **实现4**: crates/ui/src/menu/context_menu.rs:13
  - 模式：`.context_menu(...)` 只在命中当前元素 hitbox 且为右键时触发
  - 可复用：无需自建弹层，直接复用 `PopupMenu`
  - 需注意：若父子都绑定右键菜单，需要通过 hitbox 遮挡避免父级误触发

- **实现5**: crates/story/src/stories/menu_story.rs:240
  - 模式：整块区域绑定右键菜单，用于“空白区右键”场景
  - 可复用：区域级 context menu 的挂载方式
  - 需注意：需要保证文件项区域不会同时命中父级菜单

### 2. 项目约定
- **命名约定**: Rust 函数与变量使用 `snake_case`，类型与枚举使用 `CamelCase`
- **文件组织**: 视图本地交互逻辑尽量留在对应面板文件内；SFTP 双面板动作通过 `FileListPanelEvent` 回发
- **导入顺序**: 标准库、三方库、项目内模块分组导入，保持现有排序风格
- **代码风格**: 小步修改、在现有 builder 链中补行为，避免引入新的状态管理结构

### 3. 可复用组件清单
- `crates/ui/src/menu/context_menu.rs`: 通用右键菜单挂载机制
- `crates/ui/src/menu/popup_menu.rs`: `PopupMenu` 与 `PopupMenuItem` 菜单项构造
- `crates/sftp_view/src/file_list_panel.rs`: 文件列表行渲染、事件枚举、文件项菜单
- `crates/terminal_view/src/sidebar/file_manager_panel.rs`: 侧边栏文件管理器菜单与目录操作

### 4. 测试策略
- **测试框架**: Rust `cargo test`
- **验证方式**: 先执行 `cargo fmt --check`，再执行针对 crate 的 `cargo check`
- **参考文件**: 当前 UI 文件缺少现成单测，主要依赖编译验证与行为推理
- **覆盖要求**: 至少验证新增菜单构建、事件类型与调用路径可通过编译

### 5. 依赖和集成点
- **外部依赖**: `gpui`、`gpui_component`、`rust_i18n`
- **内部依赖**: `sftp_view` 依赖 `FileListPanelEvent` 与 `SftpView` 订阅；`terminal_view` 直接调用 `FileManagerPanel` 内部方法
- **集成方式**: 右键菜单触发后通过回调执行视图更新或 `cx.emit(...)`
- **配置来源**: 文案来自各 crate 的 `locales/*.yml`

### 6. 技术选型理由
- **为什么用这个方案**: 复用现有 `.context_menu(...)` 和 `PopupMenu` 体系，保持交互风格一致
- **优势**: 改动范围小，不新增状态机；可同时补齐 `..` 与空白区两类缺失场景
- **劣势和风险**: 若父子元素同时响应右键，可能出现菜单竞争，需要依赖 `occlude()` 避免命中穿透

### 7. 关键风险点
- **事件命中**: 空白区菜单挂在外层时，文件行必须遮挡父级 hitbox
- **边界条件**: 根目录下没有 `..` 行；空列表时仍需能在空白区弹菜单
- **性能瓶颈**: 仅新增菜单 builder 与 hitbox 行为，无明显性能影响
- **安全考虑**: 本任务不涉及认证、权限或网络协议变更
