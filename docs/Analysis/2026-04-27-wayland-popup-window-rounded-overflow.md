# Wayland `popup_window` 圆角背景外溢排查记录

## 先做什么

如果 Wayland 下的 popup 四角出现背景溢出，先检查 popup 内容视图自己的背景层，不要先去改窗口协议层。

优先顺序：

1. 检查具体 popup 内容视图最外层是否有 `size_full().bg(...)`
2. 检查 `TitleBar`、内容根容器、footer 是否各自直接触达四角
3. 只有内容层修正后仍有残留时，再检查 Wayland `blur_region`、`opaque_region`、dialog/modal 行为

## 这次真正的根因

这次问题长期没修掉，不是因为没有给 popup 壳层加圆角，而是因为这个前提本身不成立：

- `PopupWindowView` 的 `.rounded(...).overflow_hidden()` 只能约束自己的背景和矩形 overflow
- GPUI 当前 `overflow_hidden` 只支持矩形 content mask
- 子视图不会因为父级是圆角就自动被裁成圆角

结果就是：

- popup 壳层自己看起来是圆角
- 但内部真实内容视图如果最外层还在 `size_full().bg(...)`
- 或者标题栏、footer 自己还在画整块矩形背景
- 四角仍然会被这些子层直接画满

## 这次命中的修复方式

本次有效修复不是继续调整 Wayland 参数，而是把真正会碰到四角的背景层各自圆角化：

- `TitleBar` 自己带上圆角
- popup 内容根容器自己带整体圆角
- 有背景的 footer 自己带下圆角

对应实现原则：

- popup 壳层负责统一边框、外框背景、阴影
- 具体内容视图负责让自己的背景层与圆角几何保持一致
- 不要依赖父级 popup 壳层替子视图完成圆角裁切

## 这次首先暴露问题的窗口

- 恢复弹窗
- 新建 LLM 提供商弹窗
- 新建凭证弹窗

这些窗口都有同一个高风险结构：

- 顶层 `v_flex().size_full().bg(...)`
- 顶部 `TitleBar`
- 底部按钮区 `footer` 自己再画一层背景

在当前 GPUI 约束下，这种结构如果不分别处理圆角，就很容易在 Wayland 下露出四角。

## 后续处理建议

以后遇到同类 popup 问题，先按下面顺序做：

1. 看内容视图最外层是不是自己画了整块背景
2. 看标题栏、footer 是否直接接触四角
3. 优先改内容层圆角几何
4. 最后才看 Wayland 区域、blur、modal 行为

如果一开始就把重点放在协议层，通常会产生很多无效试错。

## 这次的验证方式

先手工复测最容易暴露问题的 popup 四角，再做定向编译：

```bash
cargo check -p one-core
cargo check -p main
```

如果编译通过且四角不再出血，基本可以确认命中了这条根因链。
