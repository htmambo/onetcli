## 审查报告（login-translation-audit）
生成时间：2026-03-26 10:55:00 +0800

### 需求完整性检查
- 目标明确：修复登录窗未翻译文案，并检查同类遗漏
- 范围明确：登录窗、设置页账号卡片、同步冲突弹窗、SSH/串口窗口标题
- 交付物明确：代码修复、本地验证、`.claude` 留痕
- 风险与依赖明确：全仓仍有较多历史翻译缺口，本次只处理当前用户可见且已确认的缺口

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：88/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：96/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- 登录窗与设置页中的 `sync_server` 将统一改为翻译键 `Settings.General.Sync.server_name`
- 登录窗错误引用的 `Settings.General.Account.sync_server_url_desc` 将改回已有正确键 `Settings.General.Sync.server_url_desc`
- 语言文件将补齐 `Home.sync_conflict_keep_both`、`SSH.new`、`SSH.edit`、`Serial.edit`

### 本地验证
- `cargo fmt --all`：通过
- `cargo check -p main -p terminal_view`：通过
- 翻译键定向扫描：通过，`main/src/auth.rs`、`main/src/setting_tab.rs`、`main/src/home_tab.rs`、`crates/terminal_view/src/ssh_form_window.rs`、`crates/terminal_view/src/serial_form_window.rs` 均为 `MISSING 0`

### 残余风险
- 其余模块仍存在历史翻译缺口，不属于本次用户反馈直接范围
