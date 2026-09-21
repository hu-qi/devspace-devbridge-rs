# DevBridge CLI (Rust)

DevBridge CLI 已使用 Rust 2024 edition 重构。

## 功能

- API Key 登录、状态检查和注销。
- 隧道创建、查询、更新、删除、token 和默认隧道管理。
- 端口创建、查询、更新、删除与匿名访问控制。
- 配额查询。
- HTTP echo 与 URI ping 调试。
- Host / Connect 数据面通过独立 transport 模块隔离。

## 构建

    cargo build --release
    cargo test --all-features
    cargo clippy --all-targets --all-features -- -D warnings

## 常用命令

    devbridge auth login --api-key <KEY>
    devbridge tunnel list
    devbridge tunnel create <name>
    devbridge tunnel set <tunnel-id>
    devbridge port create -p 8080
    devbridge limits
    devbridge echo -p 8080
    devbridge ping https://example.com

## 配置

配置文件仍为 `~/.huawei/devbridge/config.yaml`。支持 `DEVBRIDGE_API_KEY`、`DEVBRIDGE_API_BASE`、`DEVBRIDGE_CLUSTER_ID`。

## CI/CD

Pull Request 会在 Linux、macOS、Windows 上执行格式、Clippy、测试、Release 构建和安全审计；版本 tag 触发多平台 GitHub Release。