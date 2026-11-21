# P1 验收与运行

## 运行命令

```sh
cargo run --locked -p rustcraft-host --bin rustcraft-server -- --seed 42 --ticks 5
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --probe
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --smoke
cargo test --locked --workspace --all-features
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
```

服务端在指定 Tick 数后退出。`--probe` 检查 Vulkan 加载器与逻辑设备。`--smoke` 在 Linux 桌面上创建窗口、提交清屏命令并在首帧呈现后退出。窗口验证以日志 `event=vulkan_first_frame_presented` 和退出码 0 为准。

## 验收记录

| 验收项 | 证据 | 状态 |
| --- | --- | --- |
| 初始化世界与冻结注册表 | 服务端打印 `world_initialized`，内建模组测试覆盖顺序、缺失依赖、版本冲突和循环 | 通过 |
| 固定 Tick 与关闭 | `--seed 42 --ticks 5` 依次提交 Tick 1–5，打印 `world_stopped` | 通过 |
| 区块与版本 | 测试覆盖调色板位宽扩展、跨字读写、快照隔离与重新激活后的旧版本拒绝 | 通过 |
| 确定性与权威提交 | 以同一种子重演并逐 Tick 比较回执和快照，覆盖乱序输入、重复操作、会话和能力检查 | 通过 |
| 实体与本地消息 | 测试覆盖稳定实体 ID、组件 Schema 和对象版本校验、进程内命令及回执 | 通过 |
| 独立服务端 | 服务端以默认 feature 构建，依赖树无窗口与 Vulkan crate | 通过 |
| Vulkan 设备初始化 | Linux 上通过 SwiftShader 创建逻辑设备 | 通过 |
| Vulkan 窗口首帧 | 客户端已构建，当前执行沙箱连接 Wayland 与 X11 套接字均返回权限错误 | 待在可连接桌面的 Linux 会话验证 |
