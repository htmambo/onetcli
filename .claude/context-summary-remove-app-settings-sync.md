## 项目上下文摘要（remove-app-settings-sync）
生成时间：2026-03-26 10:05:22 +0800

### 1. 相似实现分析
- **实现1**: `main/src/app_settings_sync.rs`
  - 模式：应用设置作为一个额外 `SyncTypeHandler` 接入 `SyncEngine`
  - 可复用/可移除点：完整识别应用设置同步的模型映射、上传回调和测试边界
  - 需注意：删除后要同步清理 `main/src/main.rs` 的模块声明

- **实现2**: `main/src/home_tab.rs:590-675`
  - 模式：首页常规同步和冲突解决都会额外 `register_type(AppSettingsSyncType)`
  - 可复用/可移除点：应用设置同步并不在 `SyncEngine::new()` 默认处理器内，而是由应用层动态追加
  - 需注意：除了常规同步，还要清理冲突解决时的额外注册

- **实现3**: `main/src/setting_tab.rs:600-880` 与 `main/src/home/home_tabs.rs:70-160`
  - 模式：设置面板和终端事件在每次保存后都会调用 `trigger_app_settings_sync(cx)`
  - 可复用/可移除点：本地设置保存与界面联动本身独立存在，去掉同步触发不会影响本地生效
  - 需注意：要保留终端设置本地广播和 `sync_server_url` 的本地生效逻辑

### 2. 项目约定
- **命名约定**: 设置全局状态统一放在 `AppSettings`
- **文件组织**: 同步接入点在 `home_tab.rs`，设置保存逻辑在 `setting_tab.rs`
- **代码风格**: 优先删除整条链路，而不是留下空实现或条件分支

### 3. 可复用组件清单
- `main/src/setting_tab.rs`: 设置读取/保存与全局应用
- `main/src/home/home_tabs.rs`: 终端设置广播逻辑
- `main/src/home_tab.rs`: 云同步启动入口

### 4. 测试策略
- **测试框架**: Rust 编译检查与 core 单元测试
- **测试模式**: 删除链路后做引用清理检查，避免残留符号
- **参考命令**: `cargo check -p one-core -p main`、`cargo test -p one-core --lib`

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**:
  - `main/src/main.rs` 负责声明同步模块
  - `main/src/home_tab.rs` 负责注册额外同步类型
  - `main/src/setting_tab.rs` 与 `main/src/home/home_tabs.rs` 负责设置变更触发
  - `crates/core/src/cloud_sync/models.rs` 提供同步类型常量

### 6. 技术选型理由
- **为什么用这个方案**: 用户要求移除“应用设置”的同步操作，最直接且一致的实现就是删除该同步类型和所有触发入口，只保留本地设置行为
- **优势**: 行为清晰，后续不会再偷偷触发整轮云同步，也不会残留无效同步状态字段
- **劣势和风险**: 旧 `settings.json` 中可能还残留历史同步字段，但后续保存后会自然清掉

### 7. 关键风险点
- **残留调用**: 若遗漏 `trigger_app_settings_sync` 或 `register_type(AppSettingsSyncType)`，会直接编译失败或保留旧行为
- **本地行为回归**: 终端设置仍需继续同步到本地所有终端视图，不能误删这部分广播逻辑
- **工具限制**: 仓库规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供，只能基于源码检索、`cargo` 本地验证与 `.claude` 留痕完成本次修改
