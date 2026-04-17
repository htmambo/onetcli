## 项目上下文摘要（macos-local-fast-package）
生成时间：2026-03-23 12:37:59 +0800

### 1. 相似实现分析
- **实现1**: `script/bundle-macos.sh:1-52`
  - 模式：已有 `.app` 打包脚本只负责组装产物，不负责调用 `cargo build`
  - 可复用：`SCRIPT_DIR/PROJECT_DIR` 路径计算、`.app` 目录结构、`Info.plist` 与图标复制流程
  - 需注意：现有脚本把二进制路径写死在 `release`，且默认 target 固定为 `aarch64-apple-darwin`

- **实现2**: `script/bundle-macos-dmg.sh:1-40`
  - 模式：DMG 脚本通过参数驱动目标架构，只依赖 `target/OnetCli.app`
  - 可复用：`TARGET` 参数约定和打包完成后的产物命名
  - 需注意：若默认 target 不匹配本机架构，DMG 文件名会误导本地使用者

- **实现3**: `Cargo.toml:180-184`
  - 模式：正式发布使用单独的 `release` profile 管控优化强度
  - 可复用：通过自定义 Cargo profile 分离“正式发布”和“本地快速构建”
  - 需注意：`lto = "fat"` 与 `codegen-units = 1` 会把大量时间集中到最终 `onetcli(bin)` 链接阶段

### 2. 项目约定
- **命名约定**: shell 脚本使用 kebab-case；环境变量使用全大写蛇形
- **文件组织**: 打包脚本集中在 `script/`；构建配置集中在根 `Cargo.toml`
- **代码风格**: shell 脚本统一 `set -euo pipefail`，路径变量使用 `SCRIPT_DIR/PROJECT_DIR`
- **参数风格**: 脚本优先读取第一个位置参数，其次再走环境变量或默认值

### 3. 可复用组件清单
- `script/bundle-macos.sh`：本地 `.app` 组装逻辑
- `script/bundle-macos-dmg.sh`：本地 `.dmg` 组装逻辑
- `script/generate-macos-icon.sh`：图标重建逻辑
- `Cargo.toml` 的 `profile.release`：正式发布优化基线

### 4. 测试策略
- **验证方式**: 本地执行新脚本完成一次真实构建和 `.app` 打包
- **关键检查**:
  - `script/package-macos-local.sh` 在 macOS Intel 上默认选中 `x86_64-apple-darwin`
  - 二进制从 `target/<target>/release-fast/onetcli` 被正确复制到 `target/OnetCli.app`
  - 现有正式 `release` 打包入口不受影响

### 5. 依赖和集成点
- **外部依赖**: `cargo`、`uname`、macOS 自带 `sips`/`iconutil`/`hdiutil`
- **内部依赖**: 新脚本复用 `bundle-macos.sh` 和 `bundle-macos-dmg.sh`
- **集成方式**: 新增 `release-fast` profile，并通过环境变量 `ONETCLI_BUILD_PROFILE` 让 bundle 脚本切换二进制目录

### 6. 技术选型理由
- **为什么用这个方案**: 保持正式发布 `release` 配置不动，把“快”限定在本地新 profile 和新脚本里，避免影响 CI/正式产物
- **优势**: 本地 Intel macOS 可以直接一键构建；默认 target 自动匹配当前机器；正式发布链零侵入
- **风险**: `release-fast` 的体积和运行性能会弱于正式 `release`，只能作为本地验证与临时分发包

### 7. 关键风险点
- **兼容性**: 新脚本仅针对 macOS，本地默认 target 检测基于 `uname -m`
- **性能权衡**: `thin LTO + 更多 codegen-units + incremental` 换来构建速度，但会牺牲部分极致优化
- **工具说明**: 当前会话没有 `desktop-commander`、`context7`、`github.search_code`，本次使用本地源码检索和系统命令完成分析与验证
