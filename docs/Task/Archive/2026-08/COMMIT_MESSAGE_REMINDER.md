# Commit Message 草稿（待用户确认后由用户提交）

> ⚠️ **本文件仅作草稿保存，不参与 git 跟踪；用户实际 `git commit` 前请删除此文件**

本任务涉及两个独立 feature，建议拆为两个 commit：

---

## Commit 1：AI 终端操作员假性终结修复

```
feat(terminal): AI 终端操作员假性终结修复 + 完成判定矩阵

详细说明：
- prompt.rs 新增规则 6/7/8：text-only 不算完成；reminder 机制说明；配额耗尽视为已完成
- terminal_operator.rs 替换 text-only 终止逻辑为 13 行完成判定矩阵
  (Length → 追加截断提示；ContentFilter → fail-fast + Error 事件；
   Unknown → 兜底 break；Null 归一化；stop + had_tool + text>0 → reminder 注入)
- 新增 FinishReasonKind 枚举 + classify_finish_reason() 归一化大小写
- 6 个结构化监控埋点 (reminder_injected / length_path_triggered /
  premature_break / content_filter_triggered / null_finish_reason /
  unknown_finish_reason_break) cause 区分 quota_exhausted vs rounds_exhausted
- effective_max_reminders 钳制避免 reminder 突破 MAX_ROUNDS 硬限
- GlobalChatSettings.ai_terminal_agent_mid_session_reminders: usize (默认 1, 0 关闭)
  + AppSettings serde 字段 + sidebar 桥接 capability (CAP_MAX_REMINDERS)
- 9 个新单元测试覆盖所有矩阵关键路径 + 调整 write_tool_call_round_trip /
  write_wait_ms_is_clamped 补第三轮 task_complete (reminder 注入兜底)
- pre-existing view.rs `cx.new` 编译错误已用 trait 限定调用修复
  (<gpui::App as gpui::AppContext>::new)
- docs/Usage/AI_TERMINAL_OPERATOR.md 增加"假性终结防护"章节

技术细节：
- reminder 注入用 user message 形式（多数 OpenAI 兼容模型对 user message compliance 高于 system）
- task_complete 仍是唯一硬完成信号（语义不变）
- 评审记录：Round 1-4 plan + Round 1 review_code
  (P0-1 capability 断链 / P0-2 finish_reason 归一化 / P0-3 测试覆盖 /
   P1-1 None 路径 / P1-2 cause 区分 / P1-4 FinishReasonKind 枚举 /
   P1-5 error→warn 降级)

文件变更：
- 修改：crates/terminal_view/src/agents/prompt.rs
- 修改：crates/terminal_view/src/agents/terminal_operator.rs
- 修改：crates/terminal_view/src/agents/mod.rs
- 修改：crates/terminal_view/src/agents/tests.rs
- 修改：crates/terminal_view/src/agents/tools.rs (未改此处，i18n 不涉及 reminder)
- 修改：crates/terminal_view/src/agent_bridge/mod.rs (LastWriteLineMap prep)
- 修改：crates/terminal_view/src/agent_bridge/pump.rs
- 修改：crates/terminal_view/src/sidebar/mod.rs
- 修改：crates/terminal_view/src/view.rs (pre-existing 修复)
- 修改：crates/terminal_view/locales/terminal_view.yml (M4 3 keys)
- 修改：crates/core/src/ai_chat/mod.rs
- 修改：main/src/setting_tab/app_settings.rs
- 修改：main/src/setting_tab.rs
- 修改：docs/Usage/AI_TERMINAL_OPERATOR.md
- 修改：docs/Task/Active/PREMATURE_TERMINATION_DIAGNOSIS.md

测试状态：
- [x] 单元测试通过 (terminal_view agents: 35+9=44 个用例全绿)
- [x] fmt --check clean (cargo fmt -p terminal_view)
- [x] clippy 无新增警告 (除 pre-existing)
- [x] review_code 落地 P0×3 + P1×3 (Round 1)

相关 Issue：N/A

> OMC trailers:
> Constraint: 改动仅限 terminal_operator.rs:155-168 附近 + prompt.rs +
>             settings + i18n + 测试 + docs
> Rejected: 方案 A 激进续轮（与既有测试场景冲突）| 方案 C UI/持久化层诊断（截图已排除）
> Directive: 用户实测反馈"继续" 中断问题，task_complete 仍是唯一硬完成信号
> Confidence: 高 | 13 行矩阵 + 9 个新用例覆盖关键路径
> Scope-risk: task_complete 硬完成信号语义不变；AgentEvent::Stopped 未新增
> Not-tested: 端到端真实模型 reminder 注入（M8 待用户手动验收；mock provider 通过）
```

---

## Commit 2：read_terminal_output since_last_write 参数

```
feat(terminal): read_terminal_output 新增 since_last_write 参数

详细说明：
- 用户实测反馈：read_terminal_output 拿到"整个滚动缓冲区尾部 max_lines 行"，
  混合历史命令输出 + 本次命令输出，模型无法区分
- 新增 since_last_write: bool 参数（默认 true）：
  true  = 仅返回自上次 write_to_terminal 后的输出
  false = 读全终端尾部（保留旧行为作为 fallback）
- 底层 recovery_content(max_lines, from_line) 新增 from_line 起点参数
- terminal.rs 新增 total_line_count() API
- LastWriteLineMap: Arc<Mutex<HashMap<u64, usize>>> 挂在 TerminalBridge global 上
  按 terminal_id 独立跟踪上次 write 时的总行数
- TerminalOperatorHandle 新增 last_write_line_count(id) + record_last_write_line_count
- WriteOutcome 新增 line_count_before_write 字段（pump 在 write 成功后写入 map）
- read_terminal_output schema 描述同步更新（中英文）

技术细节：
- 与 commit 1 共用 agent_bridge/mod.rs 的 LastWriteLineMap prep（其实 prep
  字段是 commit 1 引入，commit 2 实质使用）
- multi terminal_id 独立跟踪不互相污染
- 首次 read 无 last_write_line_count 时退回到 from_line=0（全终端尾部）
- 边界保护：from_line < lines.len() 才 split_off；split_off(len) 不 panic
- 测试：4 个新单元用例 (write_then_read_returns_only_post_write_output /
  read_before_write_returns_full_terminal_tail /
  since_last_write_false_falls_back_to_full_tail /
  multi_terminal_last_write_tracked_independently)

文件变更：
- 修改：crates/terminal/src/terminal.rs (recovery_content + total_line_count)
- 修改：crates/terminal_view/src/agent_bridge/mod.rs (LastWriteLineMap +
  WriteOutcome.line_count_before_write + TerminalOperatorHandle 扩展)
- 修改：crates/terminal_view/src/agent_bridge/pump.rs (pump 写 last_write_lines)
- 修改：crates/terminal_view/src/agents/tools.rs (schema + since_last_write 解析)
- 修改：crates/terminal_view/src/agents/tests.rs (mock bridge 字段 + 4 新用例)
- 修改：crates/terminal_view/src/view.rs (recovery_content 传 from_line=0)
- 修改：docs/Task/Active/PREMATURE_TERMINATION_DIAGNOSIS.md (§1.3 + §4.E)

测试状态：
- [x] 单元测试通过 (terminal_view agents: 39 个用例全绿：35 既有 + 4 新)
- [x] fmt --check clean
- [x] clippy 无新增警告
- [x] review_code 待送（MCP 4xx 后未走，按 §1.4 Third priority 主流程继续）

相关 Issue：N/A

> OMC trailers:
> Constraint: 仅修改 agent_bridge / terminal / agents 测试 + docs，不动 view.rs 业务
> Rejected: 方案 A 重定向（需模型多一步）/ 方案 B OSC 133（二期展望）/
>           方案 C 增量模式（需维护游标）/ 方案 D 行号偏移（需先获取总行数）
> Directive: 用户提出"获取刚才执行的命令之后的输出"
> Confidence: 中-高 | 单元覆盖 4 关键场景；真实终端 OSC 133 缺失
> Scope-risk: recovery_content 签名破坏性变更（已全量同步 view.rs + pump.rs）
> Not-tested: 多 terminal_id 真实并发；ALT_SCREEN (vim/top) 模式；
>             历史超过 N 行时 last_write_lines 起点退化行为
```

---

## 不属于本任务范围（独立处理）

- `crates/ui/src/input/element.rs:848` GPUI 自身 UTF-8 wrap slice panic（macOS CoreText 后端）——独立 issue，单独立项修复。