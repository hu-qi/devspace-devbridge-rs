# Rust 重构与开发

DevBridge CLI 已迁移到 Rust 2024 edition，源码位于 `cli/`。

## 本地验证

在 `cli/` 目录依次执行：`cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-features`、`cargo build --release`。

## 模块边界

- `src/main.rs`：进程入口、参数解析、日志初始化。
- `src/commands.rs`：CLI 命令与业务编排。
- `src/api.rs`：REST 管理面客户端。
- `src/config.rs`：兼容 `~/.huawei/devbridge/config.yaml`。
- `src/transport.rs`：Host / Connect 数据面协议边界。
- `src/output.rs`：终端输出。

## CI/CD

Pull Request 会在 Linux、macOS、Windows 上执行格式、Clippy、测试、Release 构建、文档构建和依赖审计。推送 `vX.Y.Z` tag 后，Release workflow 会生成多平台二进制与 SHA-256 校验文件并发布 GitHub Release。