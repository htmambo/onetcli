# Netcatty AI 功能分析报告 — 引入 onetcli 可行性研究

**研究日期**：2026-04-08
**研究范围**：Netcatty 项目 Codex、Claude Code、AI 架构分析 + onetcli 现有 AI 能力评估

---

## 一、Netcatty AI 架构概览

Netcatty 构建了一套基于 **Electron + Node.js** 的双协议 AI Agent 集成框架：

```
┌─────────────────────────────────────────────────────────┐
│  Renderer (React)                                       │
│  AI Settings Tab / Chat UI                              │
├─────────────────────────────────────────────────────────┤
│  Main Process: aiBridge.cjs (~2400行)                   │
│  createACPProvider() — ACP Provider 创建入口            │
├─────────────────────────────────────────────────────────┤
│  ACP Layer (@mcpc-tech/acp-ai-provider)                 │
│  ├─ @zed-industries/codex-acp        (OpenAI)          │
│  ├─ @zed-industries/claude-agent-acp (Anthropic)       │
│  └─ copilot --acp --stdio             (GitHub)          │
├─────────────────────────────────────────────────────────┤
│  MCP Layer (Model Context Protocol)                     │
│  ├─ Netcatty MCP Server (反向注入，供 Agent 调用)        │
│  └─ 外部 MCP Server 连接                                │
└─────────────────────────────────────────────────────────┘
```

### 1.1 支持的 Agent

| Agent | npm 包 | 核心能力 |
|-------|--------|----------|
| **Codex** | `@zed-industries/codex-acp` | 深度 IDE 集成、登录会话管理、MCP snapshot |
| **Claude Code** | `@zed-industries/claude-agent-acp` | 代码生成、终端命令、持久化会话 |
| **Copilot** | 内置 CLI | `copilot --acp --stdio` |

### 1.2 核心文件清单

```
Netcatty/
├── electron/bridges/
│   ├── aiBridge.cjs          # AI 网关核心，统一处理 ACP 流式传输
│   ├── mcpServerBridge.cjs   # MCP Server TCP 桥接
│   └── ai/
│       ├── shellUtils.cjs     # CLI 路径解析、ANSI 清理、流式分块
│       ├── codexHelpers.cjs  # Codex 登录会话、认证缓存、错误归类
│       └── ptyExec.cjs       # PTY 执行器
├── electron/mcp/
│   └── netcatty-mcp-server.cjs  # Netcatty MCP Server (stdio transport)
└── infrastructure/ai/
    ├── types.ts              # ProviderConfig、ChatMessage、AI Settings
    ├── managedAgents.ts      # 托管 Agent 管理
    └── agentOutputParser.ts  # ACP JSON Lines 输出解析
```

### 1.3 关键技术亮点

1. **双向 MCP 集成**：Netcatty 不仅消费 Agent 能力，还反向注入 MCP Server，使 AI 能访问终端会话上下文
2. **六平台二进制自动路由**：darwin-arm64/x64、linux-arm64/x64、win32-arm64/x64
3. **双重认证**：ChatGPT Plus OAuth + API Key 环境变量
4. **Thinking 层级**：gpt-5.4 支持 low/medium/high/xhigh 四级思考深度
5. **安全多层防护**：命令黑名单 + Observer/Confirm/Autonomous 权限模式

---

## 二、onetcli 现有 AI 能力

### 2.1 LLM 层

- **Provider**：12 种（OpenAI、Anthropic、阿里云、智谱、Ollama、DeepSeek、Gemini 等）
- **核心抽象**：`LlmProvider` trait（`chat()`、`chat_stream()`、`models()`）
- **依赖**：`llm-connector` crate (v1.1.14)
- **流式处理**：`ChatStreamProcessor`，50ms 节流

### 2.2 Agent 框架

- **三级路由**：规则匹配 → 数量判断 → LLM 意图路由
- **内置 Agent**：GeneralChatAgent、SqlWorkflowAgent、ChatBiAgent
- **会话亲和性**：连续追问沿用上一轮 Agent（最多 10 轮）
- **动态能力注入**：`AgentContext.capabilities`

### 2.3 现有局限

| 维度 | 现状 | 影响 |
|------|------|------|
| MCP 协议 | 不支持 | 无法连接外部 Agent 生态 |
| Function Calling | 不支持 | Agent 工具调用依赖 prompt engineering |
| 命令安全 | 无 | AI 执行终端命令无保护机制 |
| Thinking 层级 | 不支持 | 无法控制模型推理深度 |

---

## 三、引入可行性评估

### 3.1 技术栈差异（根本性障碍）

Netcatty 的 AI 集成深度依赖 Node.js 生态（`@zed-industries/codex-acp`、`@mcpc-tech/acp-ai-provider` 等 npm 包），这些包无法直接在 Rust/GPUI 中使用。Codex 和 Claude Code 使用的是 **Zed 私有 ACP 协议**，无开源规范文档。

### 3.2 各项功能引入评估

| 功能 | 可行性 | 工作量 | 推荐优先级 | 原因 |
|------|--------|--------|------------|------|
| **命令黑名单 + 权限模式** | 高 | 1 周 | P0 | 与现有 PTY Backend 完全兼容，AgentContext.capabilities 天然支持扩展 |
| **会话持久化扩展** | 高 | 1 周 | P0 | 现有 SessionService 已具备基础，只需扩展到 Agent 层面 |
| **Thinking 层级选择** | 高 | 3-5 天 | P0 | Anthropic/OAI 支持 API 层面参数，llm_connector 可扩展 |
| **MCP Server 实现** | 中 | 2-3 周 | P1 | 将终端/数据库/SFTP 封装为 MCP Tools，工程量可控 |
| **MCP Client 实现** | 中 | 2-3 周 | P1 | 连接外部 MCP Server，需处理 STDIO 进程管理 |
| **Function Calling 模拟** | 低 | 2-3 周 | P2 | llm_connector 不支持，需在 Agent 层手动解析多 Provider 格式 |
| **Codex Agent 集成** | 低 | 4-6 周 | P3 | Zed 私有协议，需复刻整套 npm 包逻辑 |
| **Claude Code Agent 集成** | 低 | 4-6 周 | P3 | 同上，且 ACP 协议无规范文档 |
| **Copilot Agent 集成** | 中 | 2-3 周 | P3 | 非桌面 IDE 场景价值有限 |

---

## 四、实施建议

### 4.1 短期（1-2个月）— P0 功能

1. **命令黑名单 + 权限模式**（1周）
   - 创建 `crates/core/src/agent/security.rs`
   - 在 `AgentContext` 中注入 `PermissionMode` 和 `CommandBlacklist`
   - PTY 执行前调用 `check_command()`
   - UI 层添加 Observer/Confirm/Autonomous 选择器

2. **会话持久化扩展**（1周）
   - 在 `AgentContext` 添加 `session_id`
   - Agent 实现可加载/保存跨会话状态
   - 与现有 `StorageManager` 集成

3. **Thinking 层级选择**（3-5天）
   - 在 `ProviderConfig` 添加 `thinking_budget` 字段
   - 构建 `ChatRequest` 时透传给 Provider
   - UI 层添加 Thinking 层级选择器

### 4.2 中期（3-4个月）— P1 功能

1. **MCP Server 实现**（2-3周）
   - 创建 `crates/mcp/` crate
   - 实现 STDIO transport + JSON-RPC 协议
   - 将 PTY Backend、数据库查询、SFTP 操作封装为 MCP Tools

2. **MCP Client 实现**（2-3周）
   - 实现 MCP Client 连接到外部 Server
   - 通过 `AgentContext.capabilities` 注入 MCP 工具
   - 与现有 `IntentRouter` 集成

### 4.3 长期（6个月+）— P2 功能

1. **Function Calling 模拟方案**：在 Agent 层手动解析多 Provider 的 function_call 响应格式
2. **推动 llm_connector 添加 Function Calling 原生支持**

### 4.4 不推荐

- **直接移植 Codex/Claude Code**：Zed 私有协议，代价极高且依赖 Zed 生态更新
- **Copilot Agent 集成**：非桌面场景，ROI 不足

---

## 五、关键风险

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| llm_connector 不支持 Function Calling | 高 | 先用 Agent 层模拟方案，等待库升级 |
| MCP Rust 生态不成熟 | 中 | 参考 Anthropic 官方 MCP SDK 设计，实现精简版 |
| Rust-Node.js 互操作 | 高 | 不尝试直接调用 npm 包，通过 MCP/STDIO 间接集成 |
| AI 执行命令安全风险 | 高 | **必须优先实现命令黑名单** |

---

## 六、结论

Netcatty 的 Codex/Claude Code 集成**不可直接移植**到 onetcli——根本障碍是 Node.js/Electron 生态与 Rust/GPUI 的技术栈差异，以及 Zed 私有 ACP 协议的无规范实现。

**最有价值的借鉴方向**：
1. **MCP 协议支持** — 连接 onetcli 与更广泛 AI Agent 生态的桥梁
2. **命令黑名单 + 权限模式** — 任何 AI Agent 集成的安全基石
3. **Thinking 层级** — 低成本差异化能力

**核心原则**：借鉴思路，而非复制代码。
