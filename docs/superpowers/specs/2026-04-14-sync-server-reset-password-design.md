# Sync Server Reset Password Design

**Date:** 2026-04-14

## Goal

为 `sync_server/server` 增加一个仅供服务器本地执行的密码重置脚本，按登录邮箱定位用户，支持命令行直接传入新密码，也支持交互式隐藏输入新密码。

## Scope

- 新增一个 CLI 脚本用于按邮箱重置用户密码
- 复用现有 `DatabaseClient`、`env`、`hashPassword()`，不新增 HTTP API
- 重置密码后清理该用户现有登录会话，避免旧令牌继续可用
- 在 `package.json` 增加脚本入口，方便运维执行

## Non-Goals

- 不新增网页后台管理界面
- 不新增远程管理 API
- 不修改用户模型、密码哈希格式或认证协议
- 不引入邮件找回、短信验证等密码恢复流程

## Requirements

### Functional Requirements

1. 脚本通过邮箱定位用户，邮箱与登录邮箱一致。
2. 若命令行传入新密码，则直接使用该密码执行重置。
3. 若未传入新密码，则脚本进入交互输入模式，隐藏用户输入内容。
4. 新密码仍复用服务端统一密码策略与哈希逻辑。
5. 用户不存在时脚本应以非 0 退出，并输出明确错误。
6. 密码重置成功后，应删除该用户现有所有 `auth_sessions`。
7. 成功输出中不得打印明文密码。

### Operational Requirements

1. 脚本默认复用现有 `SYNC_SERVER_DB_PATH` 配置。
2. 脚本可以在源码运行模式下通过 `node --import tsx/esm` 直接执行。
3. 脚本应有稳定的 `npm` 入口，便于运维调用。

## Design Options

### Option A: Offline CLI Script (Recommended)

在 `src/scripts/reset-password.ts` 中新增一个本地管理脚本，直接读取环境配置并访问数据库，完成用户查询、密码哈希、密码更新和会话清理。

优点：

- 安全面最小，不暴露新接口
- 复用现有业务逻辑和环境配置
- 非常适合服务器维护场景

缺点：

- 只能在服务器或能访问数据库的环境中执行

### Option B: Admin HTTP Endpoint

新增一个管理员接口，通过远程请求触发密码重置。

优点：

- 远程调用方便

缺点：

- 需要额外的鉴权、权限校验和审计
- 明显扩大攻击面，不符合当前最小需求

### Option C: Direct SQL Utility

写一个尽量薄的脚本，直接执行 SQL 更新密码哈希。

优点：

- 实现很快

缺点：

- 容易绕开现有封装
- 更容易遗漏密码规则与会话清理

## Recommended Design

采用 **Option A**。

### Script Interface

命令格式：

```bash
npm run reset-password -- <email> [newPassword]
```

- 第一个位置参数是用户登录邮箱
- 第二个位置参数可选；如果缺失，则进入交互模式输入新密码

### Control Flow

1. 解析命令行参数并校验邮箱参数是否存在
2. 通过 `env.dbPath` 和 `env.migrationsPath` 初始化 `DatabaseClient`
3. 通过 `findUserByEmail(email.trim().toLowerCase())` 查询用户
4. 若用户不存在，输出错误并设置非 0 退出码
5. 获取新密码：
   - 如果位置参数给出，直接使用
   - 如果未给出，使用 `readline` 从 TTY 隐藏输入
6. 复用现有 `passwordSchema` 进行密码规则校验
7. 调用 `hashPassword(newPassword)` 生成新哈希
8. 调用 `updateUserPassword(user.id, hashedPassword)` 更新用户密码
9. 调用新数据库方法删除该用户全部会话
10. 输出成功信息并关闭数据库连接

## Code Changes

### New File

- `sync_server/server/src/scripts/reset-password.ts`
  - CLI 参数解析
  - 交互式隐藏输入
  - 调用数据库与密码工具完成重置

### Modified Files

- `sync_server/server/src/db/database.ts`
  - 新增 `deleteSessionsByUserId(userId: string)`，用于删除指定用户全部会话

- `sync_server/server/package.json`
  - 新增 `reset-password` 脚本入口

## Error Handling

- 缺少邮箱参数：输出使用方式并返回非 0
- 邮箱不存在：输出 `用户不存在`
- 新密码不符合规则：输出校验错误并返回非 0
- 非交互环境且未传新密码：输出明确提示，要求显式传入密码
- 数据库异常：打印简洁错误并返回非 0

## Security Considerations

- 不记录明文密码
- 交互输入必须隐藏回显
- 重置后清空用户现有会话，避免旧 token 继续有效
- 默认仅作为服务器本地运维脚本使用，不暴露远程入口

## Testing Strategy

- 增加针对数据库层会话清理方法的定向测试或最小脚本级验证
- 执行 `npm run check`
- 手工验证：
  - 使用旧密码登录应失败
  - 使用新密码登录应成功
  - 已登录会话应失效并要求重新登录

## Risks

- 如果脚本允许在非 TTY 下无密码参数执行，可能卡死；需要显式拦截
- 如果不清理会话，密码重置后的安全收益不完整
- 如果未来密码规则变化，脚本必须继续复用统一校验逻辑，不能复制规则
