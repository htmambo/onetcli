## 项目上下文摘要（windows-bundle-script）
生成时间：2026-03-28 19:35:17 +08:00

### 1. 相似实现分析
- 实现1：`script/bundle-macos.sh`
  - 模式：脚本先解析版本和目标平台，再校验二进制是否存在，最后整理分发目录结构。
  - 可复用：Windows 脚本同样应负责“解析版本、定位发布二进制、整理分发产物”三件事。
  - 需注意：macOS 产物是 `.app`，Windows 更适合输出裸 `exe` 和压缩包。
- 实现2：`script/bundle-macos-dmg.sh`
  - 模式：在已有 bundle 基础上再生成最终可分发包，并在开始前清理旧产物。
  - 可复用：Windows 脚本也应在 staging 目录基础上生成最终 `.zip`，并覆盖旧文件。
  - 需注意：压缩阶段必须稳定，不能让脚本虽然产物存在却以失败退出。
- 实现3：`script/install-window.ps1`
  - 模式：Windows 侧已有 PowerShell 脚本，说明新脚本应继续采用 `.ps1`，避免引入额外 shell 依赖。
  - 可复用：继续走 `winget/BuildTools/MSVC` 生态，不自造构建环境。
  - 需注意：当前会话 PATH 可能没有刷新，脚本必须主动定位 `cargo`、`cmake` 和 `VsDevCmd.bat`。
- 实现4：`main/src/update.rs:600-729`
  - 模式：Windows 更新逻辑本质上直接替换 `exe`，因此分发侧保留一个独立版本化 `.exe` 是有价值的。
  - 可复用：打包脚本同时输出便于更新服务使用的版本化 `exe`。
  - 需注意：Windows 更新链路并不要求安装器或 MSI。
- 实现5：`crates/core/src/themes.rs` 与 `main/src/main.rs`
  - 模式：图标、语言包和大多数资源已内嵌，运行时只写 `target/state.json`，不依赖外部 `themes/` 目录。
  - 可复用：Windows 包无需复制主题目录或 locales 目录。
  - 需注意：分发包只需包含主程序和附带说明文件即可。

### 2. 项目约定
- 命名约定：脚本放在 `script/` 目录，沿用 `bundle-*.sh/.ps1` 的动词命名方式。
- 文件组织：发布产物统一进入 `target/dist/windows-x64/`，避免污染仓库根目录。
- 代码风格：脚本以直接可执行为优先，参数简单、日志明确，不引入额外模块。

### 3. 可复用组件清单
- `script/bundle-macos.sh`：版本解析、二进制校验、bundle 目录创建模式。
- `script/bundle-macos-dmg.sh`：压缩包输出和旧产物覆盖模式。
- `script/install-window.ps1`：Windows PowerShell 脚本入口模式。
- `main/Cargo.toml`：版本号来源。
- `main/build.rs`：确认 Windows 二进制会内嵌图标资源。
- `main/src/update.rs`：确认 Windows 分发可直接使用裸 `exe`。

### 4. 测试策略
- 语法验证：PowerShell 解析 `script/bundle-windows.ps1` 不报错。
- 真构建验证：运行 `powershell.exe -File script/bundle-windows.ps1`，覆盖 release 构建和打包流程。
- 快速回归：在已有 release 二进制存在时执行 `-SkipBuild`，验证压缩和摘要文件输出。
- 产物验证：检查 ZIP 内部条目、`SHA256SUMS.txt`、版本化 `.exe` 和 staging 目录。

### 5. 依赖和集成点
- 外部依赖：
  - `cargo.exe`
  - `cmake.exe`
  - `VsDevCmd.bat`
  - `.NET System.IO.Compression.FileSystem`
- 内部依赖：
  - `main/Cargo.toml` 版本号
  - `target/<target>/release/onetcli.exe` 发布二进制
  - `LICENSE-APACHE`、`ONETCLI_LICENSE`、`README*.md`
- 集成方式：脚本在 MSVC 环境中调用 `cargo build --release -p main --target x86_64-pc-windows-msvc`，随后复制和压缩产物。

### 6. 技术选型理由
- 事实：Windows 更新逻辑以替换 `exe` 为核心，因此输出版本化 `.exe` 比单独做 MSI 更贴近现有系统。
- 事实：仓库已有 macOS bundle 脚本，但没有 Windows 分发脚本。
- 推论：对当前系统最务实的 Windows x64 打包形态是“版本化裸 `exe` + 带说明文件的 `zip` + SHA256 摘要”。
- 事实：`Compress-Archive` 在本机 PowerShell 5 上会对已存在文件返回脏错误，因此改用 `.NET ZipFile` 更稳定。
- 事实：脚本包含中文输出时，PowerShell 5 需要 `UTF-8 with BOM` 才能稳定解析。

### 7. 关键风险点
- 编码风险：若后续再次用无 BOM 方式保存该脚本，PowerShell 5 可能重新出现解析异常。
- 环境风险：release 构建仍依赖 MSVC、Cargo、CMake 和可写的 Cargo 缓存目录。
- 产物风险：当前脚本输出的是便携包，不含安装器、代码签名和自动上传逻辑。
