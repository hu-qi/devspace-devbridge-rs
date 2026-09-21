# Rust CLI 参考

## 认证

`devbridge auth login --api-key <KEY>`、`devbridge auth status`、`devbridge auth logout`。

## 隧道

`devbridge tunnel list`、`create`、`show`、`update`、`delete`、`delete-all`、`token`、`set`、`unset`。省略 tunnel id 时使用默认隧道。

## 端口

`devbridge port list`、`create`、`show`、`update`、`delete`。端口 `-1` 保留为后端“全部端口”哨兵值。

## 配额与诊断

`devbridge limits`、`devbridge echo`、`devbridge ping <URI>`。

## 环境变量

- `DEVBRIDGE_API_KEY`：API Key。
- `DEVBRIDGE_API_BASE`：管理面 API 根域名覆盖。
- `DEVBRIDGE_CLUSTER_ID`：集群 ID 覆盖。
