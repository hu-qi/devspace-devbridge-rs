# DevBridge

DevBridge 是面向华为云开发隧道场景的跨平台 CLI。本仓库当前以 Rust 作为唯一 CLI 实现，文档站使用 VitePress。

在线文档：<https://huaweicloud.github.io/devspace-devbridge/>

## Repository structure

- `cli/`：Rust 2024 CLI、测试与本地配置兼容层。
- `docs/`：VitePress 文档站。
- `.github/workflows/ci.yml`：Rust / 文档 / 安全审计。
- `.github/workflows/release.yml`：tag 驱动的多平台 Release。

## CLI development

进入 `cli/` 后执行：

    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features
    cargo build --release

Rust 工具链固定在 `cli/rust-toolchain.toml`。

## Documentation

进入 `docs/` 后执行：

    npm ci
    npm run docs:dev
    npm run check
    npm run build

## Release

推送 `vX.Y.Z` tag 后，GitHub Actions 会先执行完整 CI，再构建 Linux、macOS、Windows 多平台二进制并发布 GitHub Release。

## Compatibility

Rust CLI 继续使用 `~/.huawei/devbridge/config.yaml`，并保留默认隧道与 API Key 的配置语义。

## License

Apache License 2.0.