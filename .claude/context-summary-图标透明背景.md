## 项目上下文摘要（图标透明背景）
生成时间：2026-03-26 19:10:00 +0800

### 1. 相似实现分析
- **实现1**: `script/generate-macos-icon.sh`
  - 模式：从 `logo.svg` 渲染主 PNG，再缩放生成多尺寸平台图标。
  - 可复用：脚本参数约定、临时目录管理、源文件存在性校验。
  - 需注意：当前脚本直接消费 `logo.svg`，如果源文件含背景图层会把背景一并打进图标。

- **实现2**: `script/package-linux-deb.sh`
  - 模式：Linux 打包阶段固定消费 `resources/linux/onetcli-128.png`、`256.png`、`512.png`。
  - 可复用：尺寸命名规则与资源落盘路径。
  - 需注意：Linux 资源是正式打包输入，不能只改临时文件。

- **实现3**: `main/build.rs`
  - 模式：Windows 资源编译阶段固定引用 `../resources/windows/onetcli.ico`。
  - 可复用：Windows 图标入口路径。
  - 需注意：`.ico` 是最终产物，但内部包含多张 PNG 帧，需要一起校正透明背景。

### 2. 项目约定
- **命名约定**: 脚本文件使用 `generate-*` / `package-*` 的短横线命名。
- **文件组织**: 平台资源放在 `resources/<platform>/`，平台打包逻辑放在 `script/`。
- **导入顺序**: Shell 脚本统一先定义路径变量，再定义函数，最后执行主体。
- **代码风格**: Shell 脚本统一使用 `set -euo pipefail`，错误提示保持简洁直接。

### 3. 可复用组件清单
- `script/generate-macos-icon.sh`：已有的 SVG -> 平台图标导出流程。
- `script/package-linux-deb.sh`：Linux 图标尺寸与目标安装路径定义。
- `main/build.rs`：Windows `.ico` 资源入口。

### 4. 测试策略
- **验证方式**: 本地重新生成资源后，检查 Linux PNG 与 Windows ICO 内嵌 PNG 的角像素 alpha 是否为 0。
- **参考文件**: `resources/linux/*.png`、`resources/windows/onetcli.ico`
- **覆盖要求**: 覆盖 Linux 三个尺寸与 Windows 七个尺寸，确认角像素不再是纯白不透明。

### 5. 依赖和集成点
- **外部依赖**: `rsvg-convert` 用于 SVG 渲染；`python3` 标准库用于打包 `.ico` 与像素校验。
- **内部依赖**: `script/bundle-macos.sh` 会调用 `script/generate-macos-icon.sh`。
- **集成方式**: 平台脚本直接消费 `logo.svg`，生成落地资源文件。
- **配置来源**: 资源路径由脚本常量和 `main/build.rs` 固定约束。

### 6. 技术选型理由
- **为什么用这个方案**: 直接复用现有 `logo.svg` 与平台资源路径，变更范围最小且可重复执行。
- **优势**: 不需要引入新设计稿，不改变打包入口，后续可用脚本重复生成。
- **劣势和风险**: 需要在导出时精准移除背景矩形，避免误删其他图层。

### 7. 关键风险点
- **边界条件**: Windows `.ico` 必须保留多尺寸帧，否则会影响不同缩放级别显示。
- **性能瓶颈**: 仅为一次性资源导出，可忽略。
- **一致性风险**: 如果只改 Linux PNG 而不改 Windows `.ico`，平台资源会继续不一致。
- **工具缺口**: 仓库规范优先的 `desktop-commander`、`sequential-thinking`、`context7`、`github.search_code` 在当前运行环境不可用，本次以本地 shell 与代码证据替代并留痕。
