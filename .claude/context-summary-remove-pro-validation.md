## 项目上下文摘要（remove-pro-validation）
生成时间：2026-03-24 15:56:22 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:319`
  - 模式：云同步入口先做功能门禁，再检查登录态、冲突状态和密钥状态
  - 可复用：`trigger_sync` 的统一同步入口
  - 需注意：只要去掉这里的 License 门禁，UI 和自动同步就会直接放行

- **实现2**: `main/src/home_tab.rs:717`
  - 模式：恢复会话、OTP 登录成功后，都会拉订阅并回写 `LicenseService`
  - 可复用：登录成功后的状态刷新位置
  - 需注意：如果默认开启 Pro，这两处订阅同步应直接删掉，避免保留无意义网络依赖

- **实现3**: `main/src/license.rs:1`
  - 模式：主程序通过独立模块初始化全局 License 服务，并提供升级 Pro 对话框
  - 可复用：`init(cx)` 这个应用启动接入点
  - 需注意：如果默认开启完整功能，`license` 模块应退化为兼容层，而不是继续做验证逻辑

- **实现4**: `main/src/setting_tab.rs:720`
  - 模式：设置页账户区域暴露“导入离线 License”和登出时清除 License
  - 可复用：账户相关操作入口
  - 需注意：去掉 Pro 校验后，这里继续保留离线 License 入口会让产品语义混乱

- **实现5**: `crates/core/src/license/service.rs:23`
  - 模式：核心层通过 `LicenseService` 做在线/离线/缓存混合校验，再由 `is_feature_enabled` 决定功能
  - 可复用：既有 `LicenseInfo` / `Feature` / `PlanTier` 数据类型
  - 需注意：要把这里改成“默认返回 Pro”，否则未来只要还有旧调用路径，仍会出现隐式降级

### 2. 项目约定
- **命名约定**: UI 编排在 `main/src/*`，核心能力在 `crates/core/src/*`
- **文件组织**: `main/src/license.rs` 是启动接入点；`main/src/home_tab.rs` 是同步按钮与登录编排；`crates/core/src/license/*` 是核心 License 兼容层
- **代码风格**: 以最小变更保留接口兼容，优先删门禁、删入口、保留类型而不大规模重构调用方

### 3. 可复用组件清单
- `HomePage::trigger_sync`：同步统一入口
- `AuthService::try_restore_session` / `verify_otp`：登录后的状态回流入口
- `LicenseService`：可保留原接口名，但修改行为为默认 Pro
- `main/src/license.rs::init`：保留应用启动接入点，改为空兼容层

### 4. 测试策略
- **核心验证**:
  - `cargo test -p one-core license:: --lib`
  - `cargo check -p main`
- **文本校验**:
  - 检索升级对话框、离线 License、功能门禁、订阅更新入口是否已清理

### 5. 依赖和集成点
- `main/src/onetcli_app.rs` 仍会调用 `crate::license::init(cx)`，所以 `main/src/license.rs` 不能直接删文件
- `home_tab` 与 `setting_tab` 是 License UI 入口的主要调用方
- `one_core::license` 仍被 `cloud_sync` 层类型签名引用，因此保留模块接口比大删大改更稳妥

### 6. 技术选型理由
- **为什么不直接删掉整个 `one_core::license` 模块**: `CloudApiClient` 仍引用 `SubscriptionInfo`，且现有类型与测试依赖该模块
- **为什么把 `LicenseService` 改成默认 Pro**: 可以保证遗留调用路径不会再把功能降级
- **为什么移除设置页离线入口**: 用户已明确要求移除所有 Pro 验证相关内容

### 7. 关键风险点
- **残留入口风险**: 如果只改核心层，不改 UI，用户仍会看到升级/导入 License 的历史入口
- **残留门禁风险**: 如果只改 UI，不改核心层，未来新增调用方仍可能走旧验证逻辑
- **兼容风险**: 完全删除 `license` 类型会波及 `CloudApiClient` 和现有测试，因此本次采用“保留接口、改默认行为”的兼容方案
