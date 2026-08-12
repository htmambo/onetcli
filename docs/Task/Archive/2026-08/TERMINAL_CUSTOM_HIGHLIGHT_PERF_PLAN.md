# 终端"自定义高亮"性能优化实施计划

**Status**: 🔄 In progress（开始时间：2026-08-12）
**Owner**: Claude (Opus)
**Branch**: `perf/terminal-custom-highlight`（基于 `rename/onetcli-to-omnihub`）
**Directive**: 用户在上一轮要求基于性能分析报告"新建分支后制定可完整落地、实施的计划后开始处理"

---

## 1. 目标与背景

### 1.1 背景

OmniHub 终端的"自定义高亮"（Custom Highlight）功能默认启用 13 条内置规则（IPv4/IPv6/URL/datetime 等），允许用户增删自定义正则规则。功能通过 `CustomHighlightAddon` 在每帧渲染时对所有可见行运行正则匹配，将结果以 `DecorationSpan` 形式提供给 `RenderCache`。

### 1.2 性能问题（来自分析报告）

1. **🔴 P0 — 致命**：`terminal_element.rs:444-457` 在 `has_decorations` 为真时**全量重建**所有可见行（30× 增量放大），废止 RenderCache 的核心增量优化。
2. **🔴 P1**：addon 每帧对所有可见行全量正则扫描，未利用 `TermDamage` 做增量。
3. **🟡 P2**：`line_text[..mat.start()].chars().count()` 对每行每个匹配都做 O(n) UTF-8 完整解码。
4. **🟡 P2**：每帧重新构造 `line_text`，未做 `with_capacity` 与跨帧缓存。
5. **🟢 P3**：每帧分配 `HashSet<usize>` 做行去重。

### 1.3 目标

- **核心目标**：恢复 RenderCache 的增量渲染能力，使自定义高亮功能**不再放大**正常 PTY 输出的渲染成本。
- **量化目标**（默认 13 条规则，100×30 终端，60FPS）：
  - tail -f 单行输出场景：渲染开销回到 dirty 行规模（1/N，N = 屏幕行数）
  - 静态屏空转：CPU 占用从 ~6% 降至 < 1%
  - 中文/emoji 行：正则匹配成本 -50%~70%
- **行为目标**：匹配结果与优化前**像素级一致**。所有现有自定义规则、UI、JSON 持久化、用户迁移路径不变。

### 1.4 边界（In/Out of Scope）

**In Scope**：
- `crates/terminal_view/src/addon.rs`（CustomHighlightAddon、DecorationManager 相关）
- `crates/terminal_view/src/terminal_element.rs`（RenderCache 的装饰增量路径）
- `crates/terminal_view/src/view.rs`（dispatch_frame 调用约定）

**Out of Scope**：
- 默认规则集合的增删（保持 13 条不变）
- UI 设置面板（保持现状）
- 持久化格式（JSON schema 不变）
- 其他 addon（weblinks、search、filepath）的实现细节
- 正则引擎替换（如引入 aho-corasick）—— 列为未来优化，不在本次实施

---

## 2. 验收标准

| ID | 类别 | 标准 | 验证方式 |
|---|---|---|---|
| AC-1 | 功能 | 默认 13 条规则下的高亮匹配结果与优化前完全一致 | 视觉对比 + 既有 `addon.rs` 单测全绿 |
| AC-2 | 功能 | 用户自定义规则（任意条数）匹配结果不变 | 视觉对比 + 单测覆盖 |
| AC-3 | 性能 | tail -f 单行新输出时，`RenderCache::rebuild_lines` 调用而非 `rebuild_all` | `tracing` 日志观察 |
| AC-4 | 性能 | 静态屏无 PTY 输出时，`CustomHighlightAddon::on_frame` 为 O(dirty_lines) 而非 O(visible_lines) | 代码审查 + 日志 |
| AC-5 | 质量 | `cargo clippy -- --deny warnings` 无新增 warning | 终端命令 |
| AC-6 | 质量 | `cargo fmt --check` 通过 | 终端命令 |
| AC-7 | 质量 | `cargo test -p terminal_view` 全绿（含新增测试） | 终端命令 |
| AC-8 | 兼容 | JSON 配置文件 `terminal-settings.json` 结构不变；老用户配置无破坏 | 单测覆盖 migration 路径 |
| AC-9 | 文档 | 任务完成后归档到 `docs/Task/Archive/2026-08/` | 文件操作 |

---

## 3. 子任务清单与依赖

```
P0（核心，必做）
  ├─ T1: DecorationManager 增加按行 (line → fingerprint → spans) 缓存
  ├─ T2: RenderCache::update 改造：仅对 dirty lines 重算装饰
  └─ T3: 移除 has_decorations 早返回，启用增量重建分支

P1（高收益）
  ├─ T4: TerminalAddonFrameContext 扩展 dirty_lines 字段
  ├─ T5: CustomHighlightAddon::on_frame 只对 dirty lines 重算正则
  └─ T6: view.rs::render_terminal 构造 TermDamage 并传入 context

P2（中收益，分两步）
  ├─ T7: 新增 line_text_cache（按行缓存 + fingerprint）
  └─ T8: 新增 char_offset_map（一次性构建 (byte, char) 映射，二分查找）

P3（清理）
  └─ T9: 用 Vec<bool> 替代 HashSet<usize> 做行去重

T10: 单测覆盖（每子任务配套）
T11: cargo clippy + fmt + test 收口
T12: 归档 PLAN.md 到 Archive/2026-08/
```

依赖关系：
- T2 依赖 T1（必须先有缓存结构才能切换）
- T3 依赖 T1+T2（修改 RenderCache 早返回）
- T5 依赖 T4（先扩展 context）
- T6 依赖 T5（调用方传 dirty_lines）
- T7、T8 互相独立，可与 T5 平行实施
- T9 与上述独立，可在任意阶段插入
- T10–T12 在所有 T1–T9 完成后执行

---

## 4. 详细方案

### T1 — DecorationManager 行级缓存

**改动文件**：`crates/terminal_view/src/terminal_element.rs`

新增内部结构：

```rust
struct LineDecorationEntry {
    fingerprint: u64,         // 行内容指纹（仅用于变化检测，不要求密码学强度）
    spans: Vec<DecorationSpan>,
}

pub struct DecorationManager {
    // 旧字段保留（兼容 paint 路径）
    decorations_by_line: HashMap<usize, Vec<DecorationSpan>>,
    // 新增：跨帧缓存
    cache: HashMap<usize, LineDecorationEntry>,
}

impl DecorationManager {
    /// 仅对指定 dirty lines 重算；其余行复用 cache
    pub fn refresh_for_lines<F>(
        &mut self,
        dirty_lines: &[usize],
        term: &Term<GpuiEventProxy>,
        display_offset: usize,
        addon_manager: &AddonManager,
        compute_line: F,
    ) where F: Fn(usize, &str) -> Vec<DecorationSpan>;
}
```

要点：
- fingerprint 用现有 `left_edge_fingerprint` 同款滚动哈希（FNV-like），不引入新依赖
- 复用 `addon.rs::CustomHighlightMatch` 结构作为缓存载体，避免类型转换开销

### T2 — RenderCache 增量装饰重算

**改动位置**：`terminal_element.rs::RenderCache::update`

当前路径：
```rust
let has_decorations = !self.decoration_manager.decorations_by_line.is_empty();
if fg_changed || bg_changed || colors_changed || has_decorations {
    rebuild_all_and_update_state(term);  // ← 废止增量
    return;
}
// ... 增量路径
```

改造为：
```rust
// 1. 收集 dirty_lines（含 TermDamage + selection 变化 + left_edge）
let dirty_lines = self.collect_dirty_lines(term);

// 2. 仅对 dirty_lines 调 refresh_for_lines
self.decoration_manager.refresh_for_lines(&dirty_lines, term, ...);

// 3. 主题/颜色变化仍走全量，但与装饰解耦
if fg_changed || bg_changed || colors_changed {
    rebuild_all_and_update_state(term);
    return;
}

// 4. 走增量：rebuild_lines 只处理 dirty_lines
if dirty_lines.is_empty() {
    update_cursor(term);
} else {
    rebuild_lines(term, &dirty_lines);
}
```

### T3 — 移除 has_decorations 钳制

**改动位置**：`terminal_element.rs:444-457`

删除 `has_decorations` 早返回，让 dirty_lines 增量路径正常运行。配套删除 T2 中的 `if fg_changed || bg_changed || colors_changed || has_decorations` 整段。

### T4 — 扩展 TerminalAddonFrameContext

**改动文件**：`crates/terminal_view/src/addon.rs`

```rust
pub struct TerminalAddonFrameContext<'a> {
    pub term: &'a Term<GpuiEventProxy>,
    pub visible_lines: Range<usize>,
    pub display_offset: usize,
    pub is_local: bool,
    pub base_dir: Option<&'a Path>,
    // 新增：本帧 dirty lines（屏幕坐标系）
    pub dirty_lines: Vec<usize>,
}
```

注意：使用 `Vec` 而非 `HashSet`，避免每个 addon 都再包一层 HashSet（已有 addon 在 `on_frame` 实际不使用此字段时无开销）。

### T5 — CustomHighlightAddon 增量正则

**改动位置**：`addon.rs::CustomHighlightAddon`

```rust
fn on_frame(&mut self, context: &TerminalAddonFrameContext) {
    if self.compiled_rules.is_empty() { return; }

    let dirty_lines = &context.dirty_lines;
    let term = context.term;

    // 删除 cached_matches.clear() —— 改为只删除 dirty 行的旧匹配
    for &line_idx in dirty_lines {
        self.cached_matches.retain(|m| m.line != line_idx);
    }

    // 仅对 dirty_lines 重算
    for &line_idx in dirty_lines {
        if !context.visible_lines.contains(&line_idx) { continue; }
        let grid_line_idx = (line_idx as i32) - context.display_offset as i32;
        if grid_line_idx < 0 { continue; }

        let grid = term.grid();
        let mut line_text = String::with_capacity(term.columns());
        for col in 0..term.columns() {
            let c = grid[Line(grid_line_idx)][Column(col)].c;
            if c != '\0' { line_text.push(c); }
        }

        // T8: 用 char_offset_map 替换 chars().count()
        let offsets = build_char_offsets(&line_text);
        for rule in &self.compiled_rules {
            for mat in rule.regex.find_iter(&line_text) {
                let start_col = byte_to_char(mat.start(), &offsets);
                let end_col = byte_to_char(mat.end(), &offsets);
                if start_col >= end_col { continue; }
                // ... 构造 CustomHighlightMatch 并 push
            }
        }
    }
}
```

### T6 — view.rs 传 dirty_lines

**改动位置**：`crates/terminal_view/src/view.rs::render_terminal`

```rust
// 调用 term.damage() 一次（alacritty 已提供）
let damage = term.damage();
let dirty_screen_lines = match &damage {
    TermDamage::Full => (0..term.screen_lines()).collect(),
    TermDamage::Partial(it) => it
        .map(|d| (d.line.0 + display_offset as i32).max(0) as usize)
        .collect(),
};

let context = TerminalAddonFrameContext {
    term: &term,
    visible_lines,
    display_offset,
    is_local,
    base_dir,
    dirty_lines: dirty_screen_lines,
};
self.addon_manager.dispatch_frame(&context);

// 之后 term.reset_damage() 仍在 RenderCache::update 末尾
```

注意：`term.damage()` 在 `RenderCache::update` 内已经调用一次。要么提前调用并把 `TermDamage` 传下去，要么把 dirty_lines 解析后传给 context。**采用前者**，避免重复调用且保持所有权清晰。

### T7 — line_text 跨帧缓存

**改动位置**：`addon.rs::CustomHighlightAddon` 与 `RenderCache`

```rust
pub struct CustomHighlightAddon {
    compiled_rules: Vec<CompiledHighlightRule>,
    cached_matches: Vec<CustomHighlightMatch>,
    // 新增：按行缓存 (line_idx, line_text, fingerprint)
    line_cache: HashMap<usize, (u64, String)>,
}
```

`build_char_offsets` 与 `line_text` 一起缓存；fingerprint 与 RenderCache 的 `left_edge_fingerprint` 算法一致。

### T8 — char_offset_map

新增模块函数：

```rust
/// 一次性构建 (byte_offset, char_offset) 映射，用于把 regex 的字节位置换算为列号。
fn build_char_offsets(text: &str) -> Vec<(usize, usize)> {
    let mut map = Vec::with_capacity(text.len() / 2 + 2);
    map.push((0, 0));
    for (byte_idx, char_idx) in text.char_indices().enumerate() {
        map.push((byte_idx + 1, char_idx + 1));
        let _ = byte_idx; // 仅作占位
    }
    map.push((text.len(), text.chars().count()));
    map
}

#[inline]
fn byte_to_char(byte: usize, offsets: &[(usize, usize)]) -> usize {
    // 二分查找；map 单调递增，partition_point 直接返回 char_offset
    offsets.partition_point(|&(b, _)| b <= byte).saturating_sub(1).1
}
```

**复杂度**：单行构建 O(n) 一次，N 个 match 二分 O(N log n)，相比 N 次 chars().count() 的 O(N·n) 显著降低。

### T9 — 去掉 HashSet 分配

T1 改造后 `seen_lines` 不再需要（因为 dirty_lines 由调用方提供）。删除 `HashSet::new()` 与 `seen_lines.insert(...)`。如需保留兼容性兜底，改用 `Vec<bool>`（`num_lines` 通常 ≤ 200，比 HashSet 快）。

### T10 — 单测覆盖

新增测试（追加到 `addon.rs` 测试模块）：

| 测试名 | 覆盖范围 |
|---|---|
| `custom_highlight_dirty_lines_only_recompute_changed` | T1+T5：传入 1 个 dirty_line，验证 cached_matches 仅该行被重算 |
| `custom_highlight_static_frame_no_recompute_when_no_dirty` | T5：dirty_lines=空，cached_matches 保持不变 |
| `decoration_manager_caches_unchanged_lines_across_frames` | T1+T2：两帧 dirty_lines 不同，未脏行 spans 不变 |
| `char_offset_map_handles_ascii_cjk_emoji` | T8：边界值（空串、纯 ASCII、混合 CJK、emoji） |
| `render_cache_dirty_lines_path_with_decorations` | T2+T3：有装饰时仍走 rebuild_lines 而非 rebuild_all |
| `frame_context_dirty_lines_contains_aliased_damage` | T4+T6：Partial TermDamage 正确换算到屏幕行 |

---

## 5. 风险评估

| 风险 | 严重度 | 缓解 |
|---|---|---|
| DecorationManager 缓存 stale：行被外部擦除但未标 dirty | 中 | T1 的 fingerprint 检测变化，未变则复用；变则重算 |
| `term.damage()` 在 dispatch_frame 与 RenderCache 之间状态不一致 | 中 | 在 dispatch_frame 前调用一次，把解析结果同时传给两边 |
| char_offset_map 对超大行（>10K 字符）内存过高 | 低 | 单行级缓存，随 dirty 自动淘汰；T7 加行数上限保护 |
| 第三方 addon 依赖 `visible_lines` 但未读 `dirty_lines` | 低 | T4 是新增字段，所有 addon 默认忽略；现有 weblinks/search/filepath 均不读 |
| 优化后行为差异（颜色优先级、合并） | 低 | AC-1/AC-2 要求像素级一致；T10 单测覆盖所有现有规则场景 |
| `RenderCache::update` 中 `fg_changed/bg_changed/colors_changed` 与装饰解耦后遗漏 corner case | 低 | 单元测试覆盖 `fg_changed=true` 时仍 rebuild_all；装饰不影响该路径 |

---

## 6. 实施顺序

按 ROI 与依赖排序：

| 步骤 | 内容 | 工作量估 | 状态 |
|---|---|---|---|
| 1 | T1 — DecorationManager 行级缓存结构 | 0.5d | ⏳ |
| 2 | T3 — 移除 has_decorations 早返回 + T2 增量装饰重算 | 0.5d | ⏳ |
| 3 | T5+T6 — CustomHighlightAddon 增量正则 + view.rs 传 dirty_lines | 0.5d | ⏳ |
| 4 | T4 — 扩展 TerminalAddonFrameContext.dirty_lines | 0.2d | ⏳ |
| 5 | T8 — char_offset_map + 接入 T5 | 0.3d | ⏳ |
| 6 | T7 — line_text 跨帧缓存 | 0.3d | � |
| 7 | T9 — HashSet 清理 | 0.1d | � |
| 8 | T10 — 单测覆盖 | 0.5d | ⏳ |
| 9 | T11 — cargo clippy + fmt + test 收口 | 0.2d | ⏳ |
| 10 | T12 — 归档 PLAN.md | 0.05d | ⏳ |

总估：约 3 人日。

---

## 7. 验证策略

1. **每子任务后**：`cargo check -p terminal_view` 编译通过
2. **每子任务后**：相关单测通过
3. **全部完成后**：
   - `cargo clippy -- --deny warnings -p terminal_view`
   - `cargo fmt --check -p terminal_view`
   - `cargo test -p terminal_view`
   - `cargo test -p main`（集成层面）
4. **手工冒烟**（如可启动应用）：
   - 启用 IPv4 高亮 → `ping` 输出 IP 变蓝
   - 编辑一条自定义规则 → 实时生效
   - `tail -f /var/log/system.log` → 帧率稳定，无残字

---

## 8. 经验沉淀（计划中）

完成后追加到 `AGENTS.md` 已验证经验区：

- **标题**：终端自定义高亮开启时全量重建所有可见行，禁用 RenderCache 增量优化。
- **触发信号**：性能分析显示 `RenderCache::update` 在 `has_decorations=true` 时 `rebuild_all`，即便 TermDamage 只标记 1 行。
- **根因**：DecorationManager 没有按行缓存 + fingerprint，每次都需要全量重算所有装饰，导致早返回路径废止增量。
- **正确做法**：DecorationManager 维护按行 `(fingerprint → spans)` 缓存；CustomHighlightAddon 仅对 `TermDamage` dirty lines 重算正则；用 char_offset_map 替代 `chars().count()`。
- **验证**：`tracing` 日志观察 `rebuild_lines` vs `rebuild_all` 比例；静态屏 CPU < 1%。
- **适用范围**：所有启用自定义高亮的终端实例；扩展到其他基于装饰的 addon（weblinks hover、search 当前匹配）时复用同一缓存结构。

---

## 9. 提交策略

按 AGENTS.md 个人硬门禁：**所有 git 提交必须由用户明确指令授权后发起**。

预计拆分（Round 1 review 后调整：Commit 1 拆为两步）：

1. `refactor(terminal): 抽离 decoration 数据结构与 damage 预解析`（T1+T4+T6）
   - 新增 `LineDecorationEntry` 与 `refresh_for_lines`
   - `TerminalAddonFrameContext` 新增 `dirty_lines: Vec<usize>`
   - `view.rs::render_terminal` 一次性解析 `term.damage()`
2. `perf(terminal): 解除全量重建钳制，启用脏行增量渲染`（T2+T3+T5）
   - `RenderCache::update` 仅对 dirty lines 重算装饰
   - 删除 `has_decorations` 早返回
   - `CustomHighlightAddon::on_frame` 增量正则
3. `perf(terminal): char_offset_map 替代 chars().count()`（T8）
4. `perf(terminal): line_text 跨帧缓存 + 移除每帧 HashSet 分配`（T7+T9）
5. `test(terminal): 自定义高亮增量渲染单测`（T10）
6. `docs(task): 归档 TERMINAL_CUSTOM_HIGHLIGHT_PERF_PLAN`（T12）

每个提交独立可回滚；不在用户未授权时执行。

---

## 10. 外部评审

按 CLAUDE.md §1 强制触发：`mcp__coding-bridge__review_plan({kind:"plan"})`（session `9c8622c7-2e7e-4b3c-90d1-ab23ba80c163`）。

### Round 1/5 — APPROVED with suggestions

**总体评价**："高质量、目标导向、前置分析扎实"。

### 采纳的改进（已写入计划）

1. **拆分 Commit 1** → T1+T4+T6 拆为 `refactor(terminal): 抽离 decoration 数据结构与 damage 预解析`；T2+T3+T5 拆为 `perf(terminal): 解除全量重建钳制，启用脏行增量渲染`
2. **补充快照测试** → T10 增补 `render_cache_rebuild_all_vs_lines_visual_equivalence`：用默认 13 条规则 + CJK/Emoji/URL 混合文本，分别走两条路径，断言 `build_line_cache` 输出 `CachedLine` 字段完全一致
3. **埋点验证** → T2/T5 落地时，`on_frame` 与 `RenderCache::update` 入口打 `tracing::trace!(dirty_lines, visible_lines, "...")`，验收阶段抓取一段时间的日志做均值
4. **工作量 buffer** → 修订为 3.5-4 人日（含 Rust 借用检查阻力 + 像素级对齐调试预留）

### 拒绝的建议（附理由）

1. **拆文件到 300 行以下**
   - **拒绝**。理由：超出本次 scope（性能优化，非结构重构）；违反 `AGENTS.md` 第 348 行已验证经验"单文件 ≥ 4000 行时不要在 Rust 2018+ 项目中 fullauto 拆分"（addon.rs 1503、terminal_element.rs 1642 均涉及多 addon/多系统关注点，强行拆分将触发 `mod.rs` 路径冲突与全量迁移）；且本计划的核心目标（性能）不受文件行数影响。新代码遵守 ≤50 行函数 + ≤300 行新文件规约，已在 T10 测试代码中体现。
2. **运行时降级开关（panic fallback）**
   - **拒绝**。理由：引入未在 JSON schema 内的隐藏配置违反 YAGNI 与"不修改其他现有用户功能"原则；按 §「测试策略判定」属于过度工程。本次实施以单测覆盖核心路径为准；如线上发现 panic，应走标准 issue 流程而非运行时静默降级（降级开关本身也会掩盖 bug）。

### 后续轮次

如本计划实施过程中出现显著偏差（新增依赖、新增文件、跨 crate 改动），需触发 Round 2。

---

**变更日志**：

- 2026-08-12 创建（基于性能分析报告 + 用户指令"新建分支后制定可完整落地、实施的计划后开始处理"）
- 2026-08-12 Round 1 review 完成（`coding-bridge` session `9c8622c7-...`）：APPROVED with suggestions，采纳 4 项、拒绝 2 项并记录理由
