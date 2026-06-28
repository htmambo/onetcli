# perf/manager-lock-refactor-v3 cherry-pick 尝试报告

**状态**: ⏸ 中停(2026-06-28)
**作者**: 果农 + Claude Opus 4.8

## 一、结论

尝试在 `perf/manager-lock-refactor` 基础上新建 `perf/manager-lock-refactor-v3` 分支,把 `dev` 上独有 46 个 `fix:` + `perf:` commit 拣选过来,**46/46 全部冲突跳过**。v3 分支已删除,本次任务中停。

## 二、执行细节

### 候选列表

dev HEAD `282c2e8e` 相对 perf HEAD `3fa07b97` 的 187 个 non-merge commit,排除 vendor/zed 同步噪声,过滤 `fix:` + `perf:` 前缀 → **46 个候选 commit**(含中文括号格式 `fix(模块):`)。

### 批量执行结果

按时间顺序(2026-04 最早 → 2026-06 最新)逐个 cherry-pick,冲突策略:skip + 记录。

```
SUMMARY: applied=0 skipped=46 total=46
```

### 冲突根因

不是简单的代码行冲突,而是**结构性架构演进差异**:

1. **`main/src/onetcli_app.rs`**:本分支 perf 端已删除此文件,改为 `main/src/onetcli_app/` 模块目录。dev 上的 commit 还在修改旧路径 `onetcli_app.rs`,导致 `修改/删除` 冲突。
2. **`main/src/setting_tab.rs`**:本分支 perf 端已拆分 setting_tab(4455 行 → 2309 行,14 子轮)。dev 上的 commit 还在修改拆分前的 setting_tab.rs,导致 `内容冲突`。
3. **`crates/db_view/src/*`**:dev 上有大量 db_view 改动(extension menus / table designer / external driver),本分支 perf 端也改过同一组文件。
4. **`crates/db/src/*`**:dev 上有外部驱动 + IPC + manifest-based schema 等大量改动,本分支 perf 端也有 SQL 解析优化等改动。重叠领域广。
5. **`crates/terminal/src/*`**:dev 上有 terminal clear / lru / SSH tunnel 等改动,本分支 perf 端也有 terminal 重构。重叠领域广。

### 根本问题

`perf/manager-lock-refactor` 与 `dev` 的关系**不是"两套独立演进"**,而是**两套并行架构**:
- perf 端:setting_tab 拆分、theme 重构、wayland/IME、玻璃模糊、SSH known_hosts 等
- dev 端:MCP、extension runtime、remote desktop、database compare、IPC drivers 等

merge-base `7fd57c89` 之后,**两边各自独立演进**且**大量触及相同文件**。46 个 fix+perf 全部冲突的本质,是 **dev 上的 fix 修复的 bug 在 perf 上不存在**(因为 perf 端用了不同架构),或**修复位置已被 perf 端重构到不同路径**。

## 三、为何不继续

逐个手工解决 46 个 cherry-pick 冲突,平均每 commit 2-5 个冲突点,预计 100-250 个手工合并点,耗时 4-8 小时,且最终结果难以保证不引入新 bug。

更彻底的方案(`git merge dev`)虽然保留完整历史,但冲突规模相当(可能 200+ 手工点),且与原分支一样会被 dev 端后续演进快速覆盖。

## 四、决策

**保留 `perf/manager-lock-refactor` 原状**(HEAD `3fa07b97`)。不再尝试 cherry-pick dev 上的提交。v3 分支已删除。

## 五、未来方向

若未来希望合并 dev 与 perf 端的演进:
- **方案 A**:在 dev 上 `git merge perf/manager-lock-refactor`,利用 perf 端独有的 setting_tab 拆分、theme 重构等架构改进(可能产生架构冲突,需大量手工合并)
- **方案 B**:挑选 perf 端**真正独立、未触及 dev 热点**的提交(如 SSH known_hosts 自动接受、IME 输入法控制、玻璃模糊 UI),单独 cherry-pick 到 dev(每个 commit 范围小、冲突可控)
- **方案 C**:维持现状,让 dev 与 perf 各自演进,在合适时机通过 `git merge-base` 分析决定合并策略

## 六、原始提交清单(46 个全部 SKIPPED)

按时间顺序:

```
1. 44bfefc8 fix(http): use direct client for app user agent
2. ae22cad1 fix(release): 优化发布流程及扩展市场多源支持
3. 64fcc6ec fix(ui): 修复 SyntaxHighlighter 初始化时警告格式错误
4. d354fad4 fix(extension-runtime): 支持扩展资源相对路径及配置灵活的GitHub备用清单URL
5. 48d736f4 fix(extension-view): format status errors for display
6. 5d2c0bb fix(ui): adapt database type handling for external drivers
7. 3fb881a7 fix: remove legacy external driver id fallback
8. a74bc1c9 fix(db): 修复删除操作生成的 SQL 语句不添加 LIMIT 限制
9. c9c56ce4 fix: resolve terminal clear and connection lru
10. 2bc1d97e fix(db): reload external driver registry on demand
11. 2f4363bf fix(extension): accept wrapped IPC driver directories
12. 3f367621 fix: render external driver icons from files
13. 0893e4f9 fix: localize user-facing strings
14. 6d7040e7 fix: add linux arm64 artifact selection
15. b57719ab fix(ui): 修复滚动条显示的问题
16. de0237af fix(db_view): 修复 IpcDriverEntry 初始化缺失字段问题
17. 768afafa fix(db): 修复表触发器和检查项的表名回退处理
18. 46fc6077 fix: 禁用 TUI 内终端历史提示
19. b21cf654 fix: preserve external table design metadata
20. 0f515aed fix: switch schema for external drivers
21. 4de12d80 fix(extension-runtime): 更新扩展清单URL地址和默认下载基地址
22. af8416ea fix(release): align wasmtime for windows builds
23. d19c56f3 fix(db): 修复外部驱动加载顺序优先使用更新版本
24. c9444e1f fix(workflows): 修正 Cloudflare R2 上传步骤环境变量及变量名
25. e23a6448 fix(workflows): 删除重复的上传操作
26. ec402172 fix(workflows): 删除重复的上传操作
27. 6de21e07 fix(ipc): 修正命令路径解析逻辑以支持相对路径解析
28. d9797f7b fix(db): handle Oracle table edit literals
29. 95a07218 fix(db-view): improve table data editing and search
30. f7805214 fix(db-view): handle schema-as-database editor selection
31. ef2553a4 fix(db): persist Oracle table edits
32. eeaf8458 fix: hide plugin helper console windows
33. e55b2eba fix: tolerate missing team invitations table
34. abad0096 fix: stabilize desktop team sync
35. 66d6030d fix: preserve edits made during connection sync
36. 265d3011 fix(remote_desktop_view): 修正RGBA到BGRA格式转换 issues/76
37. 564857f3 fix(update): 修正更新对最新版本页面链接的引用
38. 034ff5f3 fix: keep connection errors actionable (#77)
39. cddac422 fix(redis): show namespace children during search (#81)
40. 6a109733 fix: separate AI reasoning from chat replies
41. a88dce61 perf: optimize remote desktop frame transport
42. 6073aceb fix(build): 修复 Linux 下 RawWindowHandle 未使用导入警告
43. 9d89e8b5 fix(windows): 恢复 GUI subsystem 并禁用 CLI 入口
44. 43cdeb47 fix(release): 调整 CI 验证并保留构建缓存
45. e674d6d6 fix(release): 同步 Cargo.lock 中 main 版本
46. 44c5cc5d fix(terminal): stop selection after missed mouse release
```

## 七、commit (本次决策记录)

见 git log `5f3d8707` 之后的 docs commit。
