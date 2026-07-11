# OnetCli → OmniHub 改名设计

**日期：** 2026-07-11  
**状态：** 已确认

## 目标

将产品名称从 OnetCli 切换为 OmniHub（暂不改仓库名）。

## 已确认决策

- 展示名：`OnetCli` → `OmniHub`
- 二进制：`onetcli` → `omnihub`
- 配置/数据目录：统一为 `omnihub`
- 方案：新名 + 启动迁移（兼容 `one-hub` / 残留 `onetcli`）
- 仓库 URL：暂不修改
- 内部 crate 名（`one-core` / `one_ui`）：暂不修改
- 密钥派生 salt/info：保持旧值，仅迁移路径

## 命名映射

| 类型 | 旧 | 新 |
|---|---|---|
| 展示名 | OnetCli | OmniHub |
| 二进制 | onetcli | omnihub |
| macOS App | OnetCli.app | OmniHub.app |
| Bundle ID | com.onetcli.app | com.omnihub.app |
| 配置目录 | one-hub / onetcli | omnihub |
| DB | one-hub.db | omnihub.db |
| 数据目录 | one-hub | omnihub |
| 安装主题 | /usr/share/onetcli/themes | /usr/share/omnihub/themes |
| IPC / PTY | onetcli* | omnihub* |
| 环境变量 | ONETCLI_* | OMNIHUB_*（读时兼容旧名） |
| 模块 | onetcli_app / OnetCliApp | omnihub_app / OmniHubApp |
| 许可 | ONETCLI_LICENSE | OMNIHUB_LICENSE |

## 迁移策略

1. 目标目录 `omnihub` 已存在 → 直接使用
2. 否则存在 `one-hub` → 迁移到 `omnihub`
3. 否则存在 `onetcli` → 迁移到 `omnihub`
4. 都没有 → 新建 `omnihub`

密钥文件只迁移路径，不改 salt。

## 验收

- 构建产出 `omnihub`
- 新用户配置落在 `~/.config/omnihub`
- 旧 `one-hub` 用户可迁移且密钥仍可用
- 打包脚本与桌面入口使用 OmniHub / omnihub
- 仓库 URL 仍可暂时保留 onetcli
