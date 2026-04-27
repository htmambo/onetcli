# upstream/dev 分支 cherry-pick 合并迁移方案

## 背景与目标

将上游 `feigeCode/onetcli.git` 的 `dev` 分支最新变更合并到当前功能分支（`remove-glass-opacity`），以同步上游的新功能、重构和修复，同时保留当前分支的特有修改（移除玻璃态透明度及相关 UI 调整）。

本次合并完成后，该方案可直接复用于其他功能分支执行相同的上游同步操作。

---

## 合并策略

### 策略选择：cherry-pick + `-X ours`

| 策略 | 原因 |
|:---|:---|
| **cherry-pick** | 当前分支与上游 dev 存在结构性差异（如 UI manifest 重构），直接 merge 会产生大量冲突，cherry-pick 可按提交粒度控制 |
| **`-X ours`** | 优先保留当前分支代码，仅在无冲突时接受上游变更。适合当前分支有独立功能（移除玻璃态）的场景 |
| **跳过高风险提交** | 对冲突过多、与当前分支架构差异过大的提交整包跳过，避免引入不可控的破坏性变更 |

### 风险说明

`-X ours` 在冲突时会**完全丢弃冲突 hunk 的上游版本**，即使该 hunk 中包含非冲突部分的有用新增代码（如新增导入、新增方法）。因此合并后必须：**逐 crate 编译验证，手动补回被误删的代码**。

---

## 上游提交清单

上游 dev 分支待合并范围：`8e37315f` ~ `81e49e7f`，共 12 个提交。

| 序号 | 提交 Hash | 提交消息 | 状态 | 说明 |
|:---:|:---|:---|:---:|:---|
| 1 | `8e37315f` | feat(db): 为 ClickHouse、DuckDB 和 MSSQL 添加完整的 UI manifest 支持 | ❌ **跳过** | 冲突过多，与当前分支数据库视图架构差异大 |
| 2 | `73461f48` | refactor(db_view): 优化上下文菜单构建逻辑并添加分组与排序 | ✅ 已合入 | 右键菜单重构 |
| 3 | `1efa71f1` | refactor(db): 优化数据库插件代码格式及本地化翻译 | ✅ 已合入 | 31 个文件，大量代码格式与本地化调整 |
| 4 | `df56b418` | fix(terminal_view): 修改 selected_index 返回类型并调整使用逻辑 | ✅ 已合入 | 终端历史提示索引类型调整 |
| 5 | `9bb2e4de` | refactor(input): 优化UTF-16到字节偏移的转换逻辑 | ✅ 已合入 | 输入法光标位置修复 |
| 6 | `5182dee2` | refactor(terminal): 重置终端表面状态以清理旧数据 | ✅ 已合入 | 新增 `reset_terminal_surface` 方法 |
| 7 | `dd51b349` | feat(remote_file_editor): 添加替换功能及快捷键支持 | ✅ 已合入 | 远程文件编辑器查找替换 |
| 8 | `18ebfe7d` | refactor(db): 优化SQL语句拆分逻辑，忽略纯注释与分隔符语句 | ✅ 已合入 | SQL streaming parser 改进 |
| 9 | `45061256` | refactor(sftp_view): 优化远程与本地目录列表管理逻辑 | ✅ 已合入 | SFTP 目录刷新逻辑重构 |
| 10 | `105ffb68` | docs(readme): 修正 README 和 README_CN 中的品牌名称格式 | ✅ 已合入 | 文档修正 |
| 11 | `53a9ffdc` | fix(file-manager): 导航失败时恢复目录并通知用户 | ✅ 已合入 | 文件管理器容错增强 |
| 12 | `81e49e7f` | chore: bump release version to v0.3.2 | ✅ 已合入 | 版本号更新 |

---

## 实施步骤

### 步骤 1：环境准备

```bash
# 确认上游远程
git remote -v
# 如不存在，添加：git remote add upstream https://github.com/feigeCode/onetcli.git

# 获取上游最新 dev 分支
git fetch upstream dev

# 确认当前分支
git branch --show-current
```

### 步骤 2：识别待 cherry-pick 提交范围

```bash
# 查看上游 dev 最新 N 个提交
git log --oneline upstream/dev | head -n 20

# 确定范围（从旧到新）
git log --reverse --oneline <start_commit>..<end_commit>
```

### 步骤 3：逐个 cherry-pick

```bash
# 按从旧到新的顺序逐个执行
git cherry-pick -X ours <commit_hash>
```

**处理冲突的规则**：
- 如果冲突简单且明确，手动解决后 `git add . && git cherry-pick --continue`
- 如果冲突过多（如提交 `8e37315f` 涉及 20+ 文件且大量架构级变更），执行 `git cherry-pick --abort` 并标记为"跳过"
- **绝不使用 `git cherry-pick --skip`**，这会跳过当前提交但继续后续提交，可能导致依赖链断裂

### 步骤 4：编译验证与修复

每 cherry-pick 3-5 个提交后执行一次编译检查：

```bash
cargo check
```

`-X ours` 最常见的副作用：**上游新增导入被丢弃**。检查模式：

```bash
# 查看当前分支与上游该提交的差异，定位被 ours 丢弃的内容
git diff <commit_hash> -- <file>
```

#### 本次合并的编译修复清单

| 错误文件 | 错误类型 | 根因 | 修复方式 |
|:---|:---|:---|:---|
| `crates/db/src/import_export/formats/{csv,json,txt,xml}.rs` | 缺失导入 `DatabasePlugin` | `-X ours` 丢弃了冲突 hunk 中的导入语句 | 手动补回 `use crate::DatabasePlugin;` 及相关导入 |
| `crates/sftp_view/src/lib.rs:1851` | 语法错误 `unexpected )` | cherry-pick 冲突解决时括号不匹配 | 重构 `if should_refresh` 块为提前 return 模式，修正括号匹配 |
| `crates/db_view/src/database_objects_tab.rs:980` | 语法错误 `unexpected }` | 冲突解决时多出一个闭合括号 | 删除多余的 `}` |
| `crates/terminal/src/terminal.rs` | 方法未找到 `reset_terminal_surface` | 编译报错时该方法实际已存在于第1719行，报错为连锁反应 | 修复其他 crate 的编译错误后自动消除 |

### 步骤 5：功能回归测试

#### 5.1 恢复功能测试（高优先级）

启动应用，验证恢复对话框及各类连接恢复：

| 连接类型 | 测试项 | 预期结果 |
|:---|:---|:---|
| SSH 终端 | 启动恢复对话框中选择恢复 | 正常恢复并连接 |
| SFTP | 启动恢复对话框中选择恢复 | 正常恢复并显示文件管理器 |
| **数据库连接** | 启动恢复对话框中选择恢复 | **正常恢复并打开数据库视图** |
| **Redis 连接** | 启动恢复对话框中选择恢复 | **正常恢复并打开 Redis 视图** |
| **MongoDB 连接** | 启动恢复对话框中选择恢复 | **正常恢复并打开 MongoDB 视图** |
| 本地终端 | 启动恢复对话框中选择恢复 | 正常恢复并保留工作目录 |

#### 5.2 本次合并引入的功能测试

| 功能模块 | 测试要点 |
|:---|:---|
| 数据库视图右键菜单 | 分组、排序、各数据库类型菜单项完整性 |
| 数据库插件 | 各数据库连接、查询执行、结果展示 |
| 终端历史提示 | 翻页、选中、回车选择 |
| 输入法 | 中文/日文/韩文输入时光标位置、选中文本 |
| 终端重连 | SSH 断线重连后缓冲区、标题、工作目录重置 |
| 远程文件编辑器 | 查找/替换框、快捷键、替换逻辑 |
| SQL 脚本执行 | 含注释、空行、分隔符的脚本执行 |
| SFTP 目录刷新 | 传输完成后本地目录自动刷新 |
| 文件管理器容错 | 输入无效路径时回退并通知 |

### 步骤 6：运行时 Bug 修复

本次合并后发现并修复的关键运行时 bug：

#### Bug：数据库/Redis/MongoDB 启动恢复失败

- **现象**：恢复对话框正常显示，点击"恢复"后 SSH/终端/SFTP 正常恢复，但数据库/Redis/MongoDB 的 tab 不出现
- **根因**：`ConnectionRestoreDialogContent::on_restore` 中传入的 `window` 是**恢复对话框窗口**。`restore_database_tab` / `restore_redis_tab` / `restore_mongodb_tab` 内部使用 `window.defer` 延迟创建 tab。但 `on_restore` 在调用恢复方法后立即同步执行 `window.remove_window()`，关闭对话框窗口，导致所有在该窗口上 defer 的任务被静默丢弃。
- **SSH/终端不受影响的原因**：它们的恢复方法不使用 `window.defer`，是同步执行的。
- **修复**：`main/src/connection_restore.rs:400`，将 `window.remove_window()` 延迟到 `window.defer` 中执行，确保恢复任务的 defer 先执行、窗口关闭后执行。

```rust
// 修复前
window.remove_window();

// 修复后
window.defer(cx, |window, _cx| {
    window.remove_window();
});
```

---

## 提交规范

### 提交信息

```
merge: 同步 upstream/dev 11 个提交并修复编译及恢复功能

- 使用 cherry-pick -X ours 策略合入 upstream/dev 的 11 个提交
- 跳过冲突过多的 8e37315f (ClickHouse/DuckDB/MSSQL UI manifest)
- 修复 -X ours 导致的导入丢失、语法错误等编译问题
- 修复数据库/Redis/MongoDB 启动恢复失败的运行时 bug
```

### 提交前检查清单

- [ ] `cargo check` 全项目编译通过
- [ ] `cargo test` 相关测试通过（如有）
- [ ] 启动恢复功能验证：数据库、Redis、MongoDB、SSH、SFTP、本地终端
- [ ] 无未预期的 `println!` 或调试代码残留

---

## 其他分支复用指南

若其他分支需要执行相同的上游同步操作：

1. **切换到自己的功能分支**
2. **执行步骤 1-3**（fetch + cherry-pick -X ours）
3. **编译检查**：重点检查 `-X ours` 丢弃的导入和新增方法
4. **运行恢复功能测试**：如果该分支也包含连接恢复功能，**必须验证**数据库/Redis/MongoDB 恢复
5. **应用步骤 6 的 Bug 修复**：如果分支代码中的 `on_restore` 存在相同的 `window.remove_window()` 同步调用模式，必须改为 defer 延迟关闭
