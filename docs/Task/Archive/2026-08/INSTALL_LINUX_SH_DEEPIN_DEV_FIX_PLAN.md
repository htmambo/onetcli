# script/install-linux.sh 适配 Deepin 25 / 补齐 X11 dev 库

**Status**: 🔄 进行中(开始时间: 2026-08-29)

## 目标

让 Deepin 25 / UOS 等基于 apt 的 Debian 衍生发行版能正确走 `script/install-linux.sh` 的 apt 分支;同时补齐 `libxcb1-dev` 与 `libxkbcommon-dev` 两个被链接器证实的缺失 X11 开发库。

## 背景

Deepin 25 在 `/etc/os-release` 中 `ID=deepin`,未被 `install-linux.sh` 的 `case` 识别,脚本落到 `*)` 兜底分支直接 `exit 1`,因此从未安装任何系统依赖。Deepin 与 Debian/Ubuntu 同源、同样使用 apt,应并入 `ubuntu|debian|linuxmint|pop` 分支。

另外,链接器报 `-lxcb` / `-lxkbcommon` / `-lxkbcommon-x11` 找不到,调查发现 `install-linux.sh` 的 apt 安装列表只列了 `libxkbcommon-x11-dev` 和 `libx11-xcb-dev`,**缺少 `libxcb1-dev`(提供 `libxcb.so` 符号链接)和 `libxkbcommon-dev`(提供 `libxkbcommon.so` 符号链接)**。

## 边界

- 单文件改动:`script/install-linux.sh`
- 改动 1:case 分支扩成 `ubuntu|debian|linuxmint|pop|deepin|uos)`
- 改动 2:apt 安装列表新增 `libxcb1-dev libxkbcommon-dev`(插入到 `libudev-dev` 之后,与其他 X11 dev 集中在一起)

## 风险

- 极低。脚本仅在新装环境的安装阶段被调用,不动现有用户文件。
- `deepin|uos` 与 `ubuntu|debian` 同源,使用相同的包名(`libxcb1-dev`、`libxkbcommon-dev`),不会引入新依赖。
- 不修改其他发行版分支(fedora / arch / opensuse)。

## 验证方式

1. `bash -n script/install-linux.sh` 语法检查。
2. 实测 Deepin 25 系统上 `bash script/install-linux.sh` 走 apt 分支不退出。
3. 安装完 dev 包后 `cargo build` 不再因 `-lxcb` / `-lxkbcommon` / `-lxkbcommon-x11` 链接失败。
4. 由外部 Review MCP 做单文件评审(CLAUDE.md §4.b 触发)。

## 执行步骤

1. 修改 `script/install-linux.sh`
2. `bash -n` 语法检查
3. 调用外部 Review MCP(kind=code)
4. 归档到 `docs/Task/Archive/2026-08/` 并更新 README 索引
5. 由用户授权 commit(个人硬门禁,不擅自 commit)

## External Review Opinion (Round 1/5)

**Verdict: NEEDS_CHANGES**(provider=coding-bridge)

评审识别出 7 条 risks,处置如下:

| # | 风险 | 处置 |
|---|---|---|
| R1 | 枚举式发行版识别仍会遗漏 Debian 衍生版(PureOS / Kali / Tails / Raspbian / Parrot / Zorin / Elementary / MX / Neon) | **本次补 9 个常见衍生 ID**;`ID_LIKE` 重构延后为单独 issue |
| R2 | `set -e` + `. /etc/os-release` 在异常 os-release 下静默退出 | 延后;预存问题,与 Deepin 修复无直接关联 |
| R3 | openSUSE 分支可能缺 `libxkbcommon-devel`(RPM 元数据历史不一致) | **本次补**;与 Deepin 同类 bug |
| R4 | `# Test on Ubuntu 24.04` 注释语义不明 | 延后;预存注释 |
| R5 | Arch `pacman -Sy` 部分升级风险 | 延后;预存问题 |
| R6 | 未启用 `set -u` / `pipefail` | 延后;预存样式 |
| R7 | `libxkbcommon-dev` 在 Ubuntu/Debian 上冗余但无害 | 保留;Deepin 上必需 |

## External Review Opinion (Round 2/5)

**Verdict: APPROVED**(provider=coding-bridge)

Round 2 确认:
- R1 partial(9 个 Debian 衍生 ID)覆盖充分且 ID 正确
- R3(openSUSE `libxkbcommon-devel`)命名规范正确
- 延后项 R2/R4/R5/R6 延后理由合理,不阻塞本次合入

剩余风险(评审员标注,延后):
- 长期应改用 `ID_LIKE` 解析替代枚举(可维护性 + 覆盖度)
- Arch `pacman -Sy` 应改 `-Syu --needed`(独立 PR)
- `. /etc/os-release` 在 set -u / pipefail 下的健壮性(独立 PR)

## 完成状态

- [x] 修改 `script/install-linux.sh`(Deepin/UOS 识别 + 9 个衍生 ID + apt 补 `libxcb1-dev libxkbcommon-dev` + openSUSE 补 `libxkbcommon-devel`)
- [x] `bash -n` 语法检查通过
- [x] Round 1/2 外部评审(verdict=APPROVED)
- [x] 归档 + 更新 README 索引
- [ ] 用户授权 commit(个人硬门禁,不擅自 commit)

## 用户侧验证(可选)

在 Deepin 25 上:
```bash
sudo apt install -y libudev-dev libxcb1-dev libxkbcommon-dev
cargo clean -p libudev-sys
cargo build
```