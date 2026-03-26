## 项目上下文摘要（login-translation-audit）
生成时间：2026-03-26 10:55:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/setting_tab.rs:582`
  - 模式：设置页同步配置使用 `Settings.General.Sync.*` 命名空间
  - 可复用：`Settings.General.Sync.server_url`、`Settings.General.Sync.server_url_desc`
  - 需注意：描述文案已经存在，登录窗应复用同一条说明，避免出现平行键

- **实现2**: `main/src/auth.rs:600`
  - 模式：登录窗同步卡片复用了设置页同一套视觉结构，但 subtitle 被直接写成 `"sync_server"`
  - 可复用：`sync_server_theme::*` 视觉样式、`t!(...)` 翻译访问方式
  - 需注意：这里错误引用了不存在的 `Settings.General.Account.sync_server_url_desc`

- **实现3**: `main/src/home_tab.rs:760`
  - 模式：冲突解决按钮通过 `Home.sync_conflict_*` 命名组渲染
  - 可复用：`Home.sync_conflict_use_cloud`、`Home.sync_conflict_use_local`
  - 需注意：`Home.sync_conflict_keep_both` 被代码使用，但语言文件未定义

- **实现4**: `crates/terminal_view/src/ssh_form_window.rs:199` 与 `crates/terminal_view/src/serial_form_window.rs:223`
  - 模式：独立弹窗标题直接引用 `SSH.*` / `Serial.*`
  - 可复用：现有 `Serial.new`
  - 需注意：`SSH.new`、`SSH.edit`、`Serial.edit` 缺失会直接显示原始 key

### 2. 项目约定
- **命名约定**: 页面级翻译以模块名前缀分组，如 `Home.*`、`Settings.General.Sync.*`
- **文件组织**: UI 组件在 `main/src` 与 `crates/terminal_view/src`，统一使用 `main/locales/main.yml`
- **导入顺序**: 沿用现有 Rust 模块导入，不因翻译修复新增无关依赖
- **代码风格**: 统一使用 `t!(...)` 返回值并在需要时 `.to_string()`

### 3. 可复用组件清单
- `main/src/setting_tab.rs`：同步设置页的正确翻译键引用
- `main/src/home_tab.rs`：首页冲突按钮命名模式
- `crates/terminal_view/src/ssh_form_window.rs`：SSH 弹窗标题翻译引用点
- `crates/terminal_view/src/serial_form_window.rs`：串口弹窗标题翻译引用点

### 4. 测试策略
- **测试框架**: 当前任务以本地编译校验和翻译键扫描为主
- **测试模式**: `cargo fmt --all` + `cargo check -p main -p terminal_view`
- **参考验证**: 额外用脚本比对 `auth.rs`、`setting_tab.rs`、`home_tab.rs` 等文件中的 `t!(...)` 是否命中语言键
- **覆盖要求**: 覆盖登录窗、设置页账号卡片、冲突对话框、SSH/串口弹窗标题

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**: `rust-i18n` 风格的 `t!` 宏、`main/locales/main.yml` 语言树
- **集成方式**: 代码引用现有翻译键，语言文件补充缺失叶子节点
- **配置来源**: `main/locales/main.yml`

### 6. 技术选型理由
- **为什么用这个方案**: 优先修正错误引用并补齐缺失键，避免在 UI 里继续出现裸字符串或错误命名空间
- **优势**: 影响面小，和现有翻译结构一致，可立即覆盖用户当前看到的问题
- **劣势和风险**: 全仓仍存在较多历史翻译缺口，本次仅收敛到登录/同步/弹窗相关用户可见路径

### 7. 关键风险点
- **边界条件**: 若某些缺失键来自其它尚未纳入的语言文件，本次扫描可能误报；已按当前仓库实际唯一 `main.yml` 处理
- **性能瓶颈**: 无
- **安全考虑**: 不涉及
