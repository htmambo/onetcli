# Project Optimization Roadmap Implementation Plan

**Goal:** 在不打断现有功能迭代的前提下，分阶段降低 `main` / `one-core` / `gpui-component` 的复杂度与构建负担，补上启动级验证链路，并明确 `sync_server` Web 端的产品边界，让后续功能开发更稳定、更容易验证。

**Architecture:** 以“先收口入口，再收敛边界，最后补验证与交付护栏”为主线推进。第一阶段只做 `main` 启动与 `OnetCliApp` 拆分；第二阶段收缩 `one-core` 与 `terminal_view` 的跨模块耦合；第三阶段为 `gpui-component` 做 feature 分层与重依赖治理；第四阶段补启动级 smoke tests、关键 CI 入口和 `sync_server` Web 端定位收敛。

**Tech Stack:** Rust workspace, GPUI, gpui-component, SQLite storage, terminal/ssh/sftp crates, TypeScript, Vue

---

## 背景

当前仓库已经具备较完整的数据库、终端、SSH/SFTP、AI、云同步能力，但结构上出现了几个持续放大维护成本的信号：

- `main/src/onetcli_app.rs` 已超过 1700 行，启动、窗口、恢复、日志、系统监控、全局状态管理聚在单文件。
- `one-core` 同时承载 storage、llm、cloud_sync、agent、popup、theme、tab persistence 等多类职责，公开 API 过宽。
- `gpui-component` 同时承担组件库、文本编辑、高亮、图表、时间控件等能力，依赖集较重，feature 分层仍偏粗。
- 测试主要集中在局部模块，缺少“应用可启动、关键初始化路径可通过”的 smoke 级验证。
- `sync_server/web` 目前仍是较薄壳层，产品定位与投入边界尚未完全明确。

本计划的目标不是一次性重写，而是做一轮可落地、可回滚、可验证的分阶段优化。

## 边界

- 本轮不改产品功能定义，不新增面向用户的大功能。
- 本轮不升级 GPUI/Git 远程依赖版本，除非某阶段为解决编译或 feature 分层阻塞而必须处理。
- 本轮不进行大规模视觉重设计；UI 相关改动仅限于拆分初始化、下沉依赖、修正 feature 边界。
- 本轮不默认重做 `sync_server` 的服务协议；只收敛其 Web 端职责和最小可用形态。
- 若某一阶段暴露出高风险共享逻辑，允许停在中间态，但必须保留编译通过和验证入口。

## 验收标准

1. `main` 启动链路按职责拆到多个模块，`main/src/onetcli_app.rs` 明显降体量，主入口职责更清晰。
2. `one-core` 的对外 API 面缩小，至少完成一轮 storage / llm / cloud_sync 访问边界收口。
3. `gpui-component` 对重量级语言高亮或编辑能力具备更明确的 feature 开关，不再默认把未来扩张都堆在一个 crate 上。
4. 增加可在本地与 CI 执行的启动级 smoke tests / targeted checks。
5. `sync_server/web` 明确定位为“完整控制台”或“最小管理面”之一，并落实到目录结构与构建入口。
6. 每阶段完成后都能提供真实验证结果，不能只停留在“结构看起来更好”。

---

## Phase 0：基线盘点与护栏

### Task 0.1：补齐基线度量与当前痛点清单

**Files:**
- Modify: `docs/plans/2026-05-22-project-optimization-roadmap.md`
- Optional: `docs/Analysis/*.md`

- [ ] 记录当前关键文件体量、核心 crate 依赖面、已有测试入口。
- [ ] 记录本轮优化涉及的高风险路径：启动、窗口恢复、主题复制、全局状态、terminal settings、cloud sync 初始化。
- [ ] 明确哪些指标用来判断优化是否有效：
  - 关键入口文件行数
  - 关键 crate `cargo check` 时间
  - 测试入口数量
  - CI 最小验证矩阵

### Task 0.2：确定分阶段验证命令

**Files:**
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Optional: `.github/workflows/*`

- [ ] 为每个阶段定义最低验证入口。
- [ ] 明确本轮的核心命令至少覆盖：
  - `cargo check -p main`
  - `cargo check -p one-core`
  - `cargo check -p terminal_view`
  - `cargo test -p main`
  - `cargo test -p gpui-component`
  - `npm run check` in `sync_server`

---

## Phase 1：拆分 `main` 启动链路与 `OnetCliApp`

### Task 1.1：收口 `main.rs` 的启动职责

**Files:**
- Modify: `main/src/main.rs`
- Create: `main/src/bootstrap/mod.rs`
- Create: `main/src/bootstrap/runtime.rs`
- Create: `main/src/bootstrap/window.rs`
- Create: `main/src/bootstrap/theme.rs`

- [ ] 将 `main.rs` 中的 CLI/update 分流、local pty host 分流、应用初始化、主题目录处理、窗口 options 构造分拆为独立 helper。
- [ ] 保持 `main.rs` 只保留启动编排，而不是承载具体初始化细节。
- [ ] 统一启动阶段错误记录方式，避免入口散落 `expect` 和匿名日志。

### Task 1.2：拆分 `OnetCliApp` 的混合职责

**Files:**
- Modify: `main/src/onetcli_app.rs`
- Create: `main/src/onetcli_app/mod.rs`
- Create: `main/src/onetcli_app/system_monitor.rs`
- Create: `main/src/onetcli_app/window_actions.rs`
- Create: `main/src/onetcli_app/tab_restore.rs`
- Create: `main/src/onetcli_app/logging.rs`
- Create: `main/src/onetcli_app/close_guard.rs`

- [ ] 先按“系统监控 / 启动恢复 / 窗口动作 / 关闭保护 / 日志”拆分，避免一次按业务页面大规模移动。
- [ ] 保持 `OnetCliApp::new`、render、全局 actions 的外部行为不变。
- [ ] 每拆出一个模块，补简短中文注释说明该模块边界。

### Task 1.3：把全局状态注册点从 UI 细节中抽离

**Files:**
- Modify: `main/src/app_init.rs`
- Modify: `main/src/onetcli_app/mod.rs`
- Modify: `main/src/home_tab.rs`

- [ ] 统一全局状态注册位置，避免部分状态在 `main.rs` 注册、部分在 `OnetCliApp` 内部注册、部分在页面首次渲染时隐式创建。
- [ ] 梳理 `GlobalDbState`、主窗口句柄、首页句柄、tab 容器、系统监控等状态的生命周期。

### Phase 1 Verification

- [ ] `cargo fmt --check`
- [ ] `cargo check -p main`
- [ ] `cargo test -p main`
- [ ] 手工验证应用可启动，覆盖：
  - 主题目录复制与 watch
  - 主窗口恢复
  - Home 页可见
  - 基础 tab 打开/关闭

---

## Phase 2：收敛 `one-core` 与跨 crate 边界

### Task 2.1：梳理 `one-core` 的公开 API 面

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/Cargo.toml`
- Optional: `crates/core/src/*`

- [ ] 盘点当前 `pub mod` 与 `pub use`，区分“对外稳定 API”和“仅供内部 crate 消费的实现模块”。
- [ ] 优先减少根模块通配式 re-export，把消费点改成更明确的子模块路径。
- [ ] 为 `agent / ai_chat / cloud_sync / storage` 标出边界说明，避免未来继续从根模块随意扩散。

### Task 2.2：拆分 storage 访问层与运行时服务层

**Files:**
- Modify: `crates/core/src/storage/mod.rs`
- Modify: `crates/core/src/storage/manager.rs`
- Modify: `crates/core/src/storage/repository.rs`
- Create: `crates/core/src/storage/runtime_paths.rs`
- Create: `crates/core/src/storage/bootstrap.rs`

- [ ] 把路径发现、目录管理、主题复制、migration 启动从 repository/manager 语义中拆开。
- [ ] 让 `StorageManager` 更聚焦于连接和仓储访问，而不是所有运行时目录杂务。
- [ ] 为 `get_config_dir / get_db_path / get_runtime_themes_dir` 提供更清晰的模块归属。

### Task 2.3：收敛 cloud sync / llm / agent 的初始化边界

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/cloud_sync/mod.rs`
- Modify: `crates/core/src/llm/mod.rs`
- Modify: `crates/core/src/agent/mod.rs`

- [ ] 明确各子系统的 `init()` 是否必须在应用启动时立即执行。
- [ ] 若可延迟初始化，增加 lazy/bootstrap helper，减少启动期全量初始化压力。
- [ ] 避免 `one-core::init()` 继续无限膨胀成“所有东西都初始化”的总开关。

### Task 2.4：审视 `terminal_view` 与 `one-core` 的交叉依赖

**Files:**
- Modify: `crates/terminal_view/src/lib.rs`
- Modify: `crates/terminal_view/src/settings.rs`
- Modify: `crates/core/src/layout.rs`
- Optional: `crates/terminal_view/Cargo.toml`

- [ ] 识别哪些常量、settings、恢复逻辑真正属于 terminal domain，哪些只是临时放在 `one-core`。
- [ ] 把 terminal 视图层对 `one-core` 的依赖收敛到必要的 layout / persistence / storage API。
- [ ] 防止 terminal 相关功能继续从 `main` 或 `one-core` 两边同时持有状态。

### Phase 2 Verification

- [ ] `cargo check -p one-core -p terminal_view -p main`
- [ ] `cargo test -p one-core`
- [ ] 运行与 storage / settings / sync 相关的定向测试

---

## Phase 3：治理 `gpui-component` 体量与重量级依赖

### Task 3.1：建立更细的 feature 分层

**Files:**
- Modify: `crates/ui/Cargo.toml`
- Modify: `crates/ui/src/lib.rs`

- [ ] 将当前过大的 UI 能力按领域划分为 feature：
  - `input-core`
  - `editor`
  - `markdown`
  - `charts`
  - `calendar`
  - `syntax-basic`
  - `syntax-extended`
- [ ] 保证默认 feature 集尽量贴近主应用真实需要，而不是未来可能用到的全部能力。
- [ ] 避免为了一个 story/example 把重量依赖默认带入所有消费者。

### Task 3.2：收敛 tree-sitter 与高亮语言装配方式

**Files:**
- Modify: `crates/ui/Cargo.toml`
- Modify: `crates/ui/src/highlighter/mod.rs`
- Modify: `crates/ui/src/highlighter/registry.rs`
- Optional: `crates/ui/src/highlighter/languages/*`

- [ ] 区分基础语言包与扩展语言包，避免单一 feature 挂载全部语言解析器。
- [ ] 若主应用只稳定依赖少数语言，优先把剩余语言改为扩展 feature。
- [ ] 保持 story/examples 可通过显式 feature 启动完整语言集。

### Task 3.3：把示例/文档消费与主应用消费隔离

**Files:**
- Modify: `crates/story/Cargo.toml`
- Modify: `examples/*/Cargo.toml`
- Optional: workspace `Cargo.toml`

- [ ] 明确 story/examples 是否需要全部 UI feature。
- [ ] 让文档与示例消费重功能 feature，主应用只启用业务所需子集。
- [ ] 若需要，新增 workspace 层说明文档，解释各 feature 的用途与组合方式。

### Phase 3 Verification

- [ ] `cargo check -p gpui-component`
- [ ] `cargo check -p story`
- [ ] `cargo test -p gpui-component`
- [ ] 至少做一次 feature 组合验证，确认默认与 full feature 都可编译

---

## Phase 4：补启动级 smoke tests 与 CI 护栏

### Task 4.1：增加启动级 smoke tests

**Files:**
- Modify: `main/src/main.rs`
- Modify: `main/src/onetcli_app/mod.rs`
- Create: `main/tests/startup_smoke.rs` 或最接近的可测试入口

- [ ] 为主题目录准备、配置目录解析、窗口参数构造、默认日志路径等可脱 UI 验证的逻辑补 smoke tests。
- [ ] 对必须依赖 GPUI 的部分，优先提炼纯函数或小型 helper，提高可测试性。
- [ ] 避免把整个应用 GUI 启动进测试，只验证关键初始化语义。

### Task 4.2：建立最小 CI 验证矩阵

**Files:**
- Modify: `.github/workflows/*`
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`

- [ ] 定义 PR 最小校验：
  - Rust format
  - 核心 crate `cargo check`
  - 核心测试子集
  - `sync_server` check
- [ ] 若全量矩阵成本过高，先以“核心主路径 + 定向 crate”落地，再逐步扩充。

### Task 4.3：补结构性回归测试点

**Files:**
- Modify: `main/src/setting_tab.rs`
- Modify: `crates/core/src/storage/*.rs`
- Modify: `crates/terminal_view/src/settings.rs`
- Modify: `crates/ui/src/theme/tests.rs`

- [ ] 围绕本轮拆分后的 helper 和服务边界补测试，而不是继续把逻辑埋回大文件。
- [ ] 优先覆盖：
  - 路径选择
  - 启动默认值
  - settings 持久化
  - feature-gated 初始化行为

### Phase 4 Verification

- [ ] `cargo test -p main`
- [ ] `cargo test -p one-core`
- [ ] `cargo test -p gpui-component`
- [ ] `cargo check --workspace`

---

## Phase 5：收敛 `sync_server` Web 端定位与目录结构

### Task 5.1：明确 Web 端定位

**Files:**
- Modify: `sync_server/README.md`
- Modify: `docs/plans/2026-05-22-project-optimization-roadmap.md`
- Optional: `docs/superpowers/specs/*.md`

- [ ] 在“完整控制台”与“最小管理面”之间明确选一条主路径。
- [ ] 给出定位后的最小页面集合、数据流和非目标范围。
- [ ] 避免 Web 端无限期停留在 Router 壳层。

### Task 5.2：按定位收敛目录与构建入口

**Files:**
- Modify: `sync_server/package.json`
- Modify: `sync_server/web/src/app/App.vue`
- Modify: `sync_server/web/src/layouts/AppLayout.vue`
- Modify: `sync_server/web/src/stores/*`
- Modify: `sync_server/web/src/components/*`

- [ ] 若选择“最小管理面”，删掉或暂停不必要的前置抽象，优先做登录/状态/统计/管理关键页。
- [ ] 若选择“完整控制台”，补上模块目录约定与路由组织，避免后续继续平铺。
- [ ] 保持与 server 端的环境、鉴权、启动命令一致。

### Phase 5 Verification

- [ ] `npm run check` in `sync_server`
- [ ] `npm run build` in `sync_server`
- [ ] 手工验证登录和至少一个核心管理流程

---

## 执行顺序建议

1. 先做 Phase 1，尽快降低 `main` 层认知负担。
2. 再做 Phase 2，把 `one-core` 的边界收紧，避免拆完入口后问题继续向 core 回流。
3. 然后做 Phase 3，为后续构建与依赖治理建立 feature 基础。
4. Phase 4 与前 3 阶段并行穿插，但最迟应在声称本轮完成前补齐。
5. Phase 5 独立收尾，不阻塞 Rust 主体结构优化。

## 风险与取舍

- `main` 与 `one-core` 的拆分如果同时推进过快，容易把“文件变小”做成“跨模块跳转变多但边界没变清楚”；因此本计划优先做职责拆分，再做 API 收口。
- `gpui-component` 的 feature 细分如果过细，会显著增加组合测试成本；因此本轮只拆真正重量级能力，不追求一次到位。
- `sync_server/web` 的定位收敛涉及产品决策；若用户需求仍在变动，可先完成目录和最小页面框架，但必须在文档中明确未决点。
- 启动级 smoke tests 需要从现有大文件里提炼纯逻辑 helper，这本身也是结构优化的一部分，不能指望最后补测试时自然出现。

## 完成定义

满足以下条件后，才可声称本轮项目优化完成：

- `main`、`one-core`、`gpui-component` 三个核心区域均完成至少一轮结构性收口并通过验证。
- 关键拆分点有对应测试或 smoke 入口，不依赖纯手工口头说明。
- `sync_server/web` 的定位和最小结构已文档化并落地到代码目录。
- README / CONTRIBUTING / 相关计划文档已同步更新，不让新贡献者继续沿旧路径扩张复杂度。

---

## Current Status

- 已完成：
  - 新建实施分支 `docs/optimization-roadmap`
  - 新增本计划文档
  - `main/src/onetcli_app.rs` 已切换为目录模块：`main/src/onetcli_app/mod.rs`
  - 已抽出第一批低耦合 helper：
    - `main/src/onetcli_app/system_monitor.rs`
    - `main/src/onetcli_app/tab_restore.rs`
    - `main/src/onetcli_app/logging.rs`
    - `main/src/onetcli_app/window_actions.rs`
    - `main/src/onetcli_app/close_guard.rs`
  - `one-core storage` 路径与主题目录职责已开始从 `manager.rs` 下沉到 `crates/core/src/storage/runtime_paths.rs`
  - README / CONTRIBUTING / CI 已补最小验证主路径

- 进行中：
  - 清理 `main/src/onetcli_app/mod.rs` 中已迁出的旧实现并接回新模块
  - 验证 `cargo fmt --all` 与 `cargo check -p main`
  - 收敛 `storage::manager` 与 `storage::runtime_paths` 的剩余边界

- 已发现问题：
  - `crates/core/src/storage/manager.rs` 原先引用的 `manager_tests.rs` 缺失，已补回最小回归测试文件
  - 当前仓库存在较多未格式化文件；本轮需要分阶段保持“局部收口 + 持续可编译”，避免一次触发全仓大面积格式噪音

- 下一步：
  - 先恢复 `main` / `one-core` 当前编译通过
  - 再继续拆 `main.rs` 的 bootstrap helper
  - 随后推进 `gpui-component` feature 收口和 `sync_server/web` 定位文档化
