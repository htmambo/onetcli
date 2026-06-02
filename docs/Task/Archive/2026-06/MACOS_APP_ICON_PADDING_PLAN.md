# macOS 应用图标内边距优化与白边修复任务计划

**状态**: ✅ 已完成 (完成时间: 2026-06-02)

## 任务目标

修复 `OnetCli.app` 应用图标在 macOS Finder / Dock / Launchpad 中显示时的两个问题：

1. **白边问题**：图标外圈出现"白边"（实际是系统桌面 / Finder 背景透出）
2. **外围空白过多**：`logo.svg` 自带圆角矩形与 macOS 系统 squircle 双重叠加后，"终端窗口"在系统圆角容器中显得过小

## 根因分析

### 白边根因

`logo.svg:35` 的外层圆角边框为 stroke-only 装饰：

```xml
<rect x="24" y="24" width="976" height="976" rx="180" ry="180"
      fill="none" stroke="url(#borderGradient)" stroke-width="48" />
```

- `stroke-width=48` 向画布外圈延伸 24px（`x=24 - 24 = 0`）
- macOS 系统 squircle（圆角半径 ≈ 22.4% × 1024 = 229px）切掉 stroke 最外圈后
- **露出的不是均匀的描边色，而是 SVG 画布的透明背景**
- 深色桌面 / Finder 背景透过透明背景呈现 = 视觉上的"白边"

### 双重圆角根因

- `logo.svg` 内部已画两层圆角矩形（`rx=120` 主体 + `rx=180` 外层边框）
- macOS Big Sur+ 再叠加一层 squircle（`rx ≈ 229`）
- 三层圆角叠加后视觉拥挤，且当前 `scale=0.95` 让内容几乎贴边

## 实现路径

**策略**：新建专供 macOS 路径的 `logo-macos.svg`，外层圆角边框改为**实心圆角背景**（不再是 stroke-only 装饰），让背景渐变延伸至画布最外圈，被系统 squircle 切掉后呈现自然的渐变衰减而非透明背景。

**约束**：
- 仅改动 macOS 路径（`script/generate-macos-icon.sh` + 新建 `logo-macos.svg`）
- 不引入 Pillow / ImageMagick 等第三方包（用 macOS 原生 `sips` + `iconutil`）
- 保留原 `logo.svg` 不变，供 Linux / Windows / 网站等场景继续使用
- 不破坏现有 `OnetCli.icns` 增量更新逻辑（`bundle-macos.sh:74-76` 依赖此行为）

## 任务分解

- ✅ T1: 新建 `logo-macos.svg` —— 外层圆角边框从 stroke-only 改为实心背景（`rx=232`，`fill="url(#bgGradient)"`）
- ✅ T2: 调整 `script/generate-macos-icon.sh` 默认源文件为 `logo-macos.svg`，并移除内层 `transform` 缩放包装
- ✅ T3: 重新生成 `resources/macos/OnetCli.icns` 并视觉验证
- ✅ T4: 归档任务文档 + 更新 `docs/Task/README.md` 索引
- ✅ T5: 修正 `script/bundle-macos.sh` 中"从 logo.svg 生成"的过时注释（codex review 发现）

## 改动内容

### 新建 `logo-macos.svg`

基于 `logo.svg` 修改：
- 删除外层 stroke 边框（`rect ... fill="none" stroke="url(#borderGradient)" stroke-width="48"`）
- 在画布最外圈新增实心圆角矩形作为背景：
  ```xml
  <rect x="0" y="0" width="1024" height="1024" rx="220" ry="220" fill="url(#bgGradient)" />
  ```
- 主体 rect（`rx=120`）、内层高光边框、终端按钮、文本、`>_` 提示符**保持原样**
- viewBox 保持 `0 0 1024 1024`

### 调整 `script/generate-macos-icon.sh`

- 默认源文件从 `logo.svg` → `logo-macos.svg`（CLI 参数优先级保留）
- 移除内联 Python 中的 `transform="translate(margin) scale()"` 包装逻辑（SVG 已内置安全边距）
- 移除 `MACOS_ICON_MARGIN` / `MACOS_ICON_SCALE` 两个参数（已不再需要）
- 保留 `sips` 多尺寸缩放 + `iconutil` 打包逻辑
- 保留 PNG IEND trailing 垃圾清理逻辑

## 验收标准

- [x] `cargo check` 通过（理论上不涉及 Rust 代码）
- [x] `bash script/generate-macos-icon.sh` 重新生成 `OnetCli.icns` 成功（668657 bytes，ic12 类型）
- [x] 1024×1024 PNG 像素采样验证：
  - 四角 (0,0)(1023,0)(0,1023)(1023,1023) 全部 RGBA(0,0,0,0) 透明 ✓
  - 边缘中点 RGB(32,32,32) 深色（rx=232 渐变延伸至边缘）✓
  - 距角 100/200/230/240/300 都有渐变像素 ✓
  - 中心 (512,512) RGB(255,255,255) 白色文字 ✓
- [x] `bash -n` 三个相关 shell 脚本语法检查通过
- [x] `logo.svg` 文件未改动（`git diff --stat -- logo.svg` 无输出）
- [x] `git status` 改动列表：`M resources/macos/OnetCli.icns`、`M script/bundle-macos.sh`、`M script/generate-macos-icon.sh`、`?? logo-macos.svg`
- [x] Linux / Windows 图标生成脚本（`generate-linux-windows-icons.sh`）**未受影响**

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| `logo-macos.svg` 视觉与 `logo.svg` 差异过大 | 仅修改外层边框为背景层，内部元素完全保持；用 Preview 并排对比验证 |
| macOS squircle 切边后圆角不自然 | 选用 `rx=220`（约 21.5% 圆角），略大于系统 22.4% squircle 的内切圆角，留出渐变过渡 |
| `OnetCli.icns` 体积变化 | iconutil 压缩稳定，差异可忽略 |
| 影响 Linux / Windows 场景 | 仅新建 `logo-macos.svg`，原 `logo.svg` 与 `generate-linux-windows-icons.sh` 不变 |
| 系统缓存的旧图标 | `killall Dock` 重建 Launchpad 缓存 |

## 备注

- 对照 `../flymd/scripts/make_icon_safearea.py` 的设计思路（PNG 透明画布 + 居中缩小），本任务采用**纯 SVG 层方案**，原因是：
  - macOS 路径不能引入 Pillow（用户选择"原生"约束）
  - `sips` 不能做"PNG 在透明画布上的居中合成"
  - 在 SVG 层把背景延伸至画布最外圈，等效于"PIL 透明画布 + 缩放" 的最终视觉效果
- 验证后将本任务的关键经验（macOS squircle 与源图自绘圆角的冲突模式）沉淀到 `AGENTS.md` 的"已验证经验"部分

## 验收结果

- 验收时间：2026-06-02
- 验收方式：像素采样（Python stdlib PNG 解码器 + sips 渲染 PNG 输出）
- 验收结论：通过
- Codex review 结论：未发现阻塞问题（指出 `bundle-macos.sh` 注释不准确，已修正）
- 落地改动：4 个文件（1 新增 + 3 修改）
- 未执行 git commit（按 AGENTS.md 个人硬门禁，需用户明确授权）
