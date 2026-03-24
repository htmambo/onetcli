# GPUI

此目录保存了 OnetCli 当前使用的 vendored `gpui` 源码副本。

- 上游仓库：`https://github.com/zed-industries/zed`
- 当前用途：为 Linux / Deepin 窗口行为修复提供可复现的本地依赖
- 保留原因：该 crate 的 `Cargo.toml` 和 crate 级文档都会引用此文件，缺失时会导致编译失败
