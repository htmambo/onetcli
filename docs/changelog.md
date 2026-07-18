---
title: 更新日志
description: 基于本地 git tag 和提交历史整理 OmniHub 最近版本与关键能力更新
---

# 更新日志

本页基于当前仓库的本地 `git tag` 和提交历史整理，适合在官网中展示最近版本的产品变化。完整发布包、安装文件和历史版本仍以 [GitHub Releases](https://github.com/feigeCode/onetcli/releases) 为准。

## 0.6.0

发布时间：2026-07-18

### Highlights

OmniHub 独立版本线的首个里程碑：吸收上游 OnetCli 分支的核心能力，完成外部驱动插件系统的可观测性加固，并引入远程桌面、端口转发、外部数据库驱动三项用户可见能力。

### New Features

- **远程桌面**：连接 VNC 兼容服务端（含 macOS 屏幕共享 / ARD），支持 Contain / Original / Cover / Fill 四种显示模式与实时键鼠转发；Provider 经 marketplace 按需安装，含 SHA-256 校验与回滚。
- **端口转发**：本地端口转发与动态 SOCKS 转发，独立视图集中管理多条规则。
- **外部数据库驱动**：通过版本化 IPC 协议加载第三方数据库引擎，含 manifest schema 校验与 marketplace 发现。
- **终端 AI**：新增命令流入口，结果以内联 transcript / flow block 呈现，优化 overlay 控件，修复 transcript 字符集重置与错误后释放终端等问题。

### Internal Improvements

- IPC 驱动协议版本门禁（major 拒绝 / minor 放行+告警 / 遗留隐式放行）。
- manifest 未知字段软告警（serde_ignored，完整字段路径）。
- 子进程 stderr 背压：单行 64KB 截断、20 行/秒速率限制、drain/log 解耦防管道阻塞、EOF 汇总。

### Versioning

OmniHub 采用独立版本线，与上游 OnetCli 项目不共用版本号，两者版本号不可比较。

## v0.4.8

发布时间：2026-06-11

这一版重点围绕快捷键、表格数据操作、SFTP 文件操作、连接安全和 Redis 入口继续增强。

- 支持自定义快捷键。
- 新增表格数据导航动作。
- 数据库视图支持根据备注搜索并显示备注信息。
- SFTP 支持多选上传入口，并新增文件多选下载、删除能力。
- 添加收藏路径和表格行高设置。
- 优化主密钥解锁流程和连接加载控制。
- 添加 Redis 标签页打开上下文函数。
- 修复表格单元格单行展示、内联编辑居中、删除行时选择与拖拽状态清理等问题。
- 修复云同步连接冲突处理的同步状态与逻辑问题。
- 调整 Redis、MongoDB 相关截图资源。
- 修复 hotkey manager 线程本地保存问题。

## v0.4.7

发布时间：2026-05-25

这一版重点补齐 MongoDB 表格化浏览与编辑，并修复数据库连接和导出稳定性问题。

- 支持 MongoDB 文档表格视图。
- 优化 MongoDB 表格预览。
- 支持 MongoDB 表格字段编辑。
- 增强 MySQL SSH 隧道认证错误提示。
- 修复 SQL 转储崩溃风险。
- 修复数据库对象视图 schema 展开问题。
- 终端新增清屏功能和本地工作目录解析。

## v0.4.6

发布时间：2026-05-21

这一版重点增强 Redis 工具页签、发布订阅和多数据库切换，并修复 macOS 发布链路。

- 新增 Redis 工具页签及相关视图功能。
- 新增 Redis 发布订阅支持及相关 UI 和交互。
- 优化 Redis 连接管理与数据库切换。
- 稳定 DuckDB 和 macOS release packaging。
- 优化 macOS 清理脚本。
- 修复 `hdiutil` DMG 临时后缀处理。

## v0.4.5

发布时间：2026-05-16

这一版重点围绕 Redis 多数据库并发连接、数据表刷新和文本编辑体验进行打磨。

- Redis 支持多数据库并发连接和操作。
- 优化数据表刷新与文本编辑器换行行为。
- 用 `LargeTextEditor` 替换 `MultiTextEditor`。
- 更新界面图片资源。

## 官网更新

2026-06-11 官网同步完成一次产品定位升级：

- 首页升级为深色 Rust 原生工具风格。
- 主标题调整为“纯 Rust 构建的高性能一体化运维工作台”。
- 强化“纯 Rust、GPUI、GPU Rendered、No WebView”的技术定位。
- 首页新增大量真实产品截图，覆盖主工作台、数据库、SSH、AI 分析、Redis、MongoDB、SFTP、远程文件编辑、ER 图和服务器监控。
- 全局 VitePress 主题色切换为统一深色体系：
  - 背景：`#0B0D13`
  - 卡片：`#131622`
  - 边框：`#222638`
  - 主色：`#E05A47`
  - 辅色：`#6366F1`
- 功能页、下载页、文档页和更新日志同步补齐新产品定位与能力说明。

## 更多版本

更早版本请查看本地 tag 或 [GitHub Releases](https://github.com/feigeCode/onetcli/releases)。
