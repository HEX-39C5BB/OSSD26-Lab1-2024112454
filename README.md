# RustCraft（C4 分支）

RustCraft 是用 Rust 开发的独立体素沙盒引擎和第一人称客户端。项目已通过 P3 本地收口验收，提供原创平坦地形、方块放置与破坏、有限库存、飞行和本地存档。世界由固定 20 Hz 的权威模拟驱动，存档记录原生规则的版本。[P3 验收清单](doc/p3-acceptance.md)

## 快速开始

从仓库根目录运行命令，使用 Rust 1.95 或以上版本。当前客户端以 Linux Wayland 下的 Intel 核显为验收基线，要求 Vulkan 1.3。NVIDIA RTX 4060 Laptop GPU 已完成窗口流送补测，X11 暂不支持。构建客户端需要 `pkg-config`、ALSA 和 PipeWire 开发库及 `libclang`。[实机记录](doc/performance.md#gentoo-expanded-results)

### 启动客户端

```sh
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client
```

首次点击窗口捕获鼠标。用 WASD 移动、鼠标观察、左键破坏、右键放置，数字键 `1`–`8` 选择原生快捷栏（Java 预设为 `1`–`9`），滚轮上下可循环切换。空格跳跃，双击空格切换飞行。飞行中空格上升、Shift 下降，下降触地切回行走。Esc 暂停并打开设置，可逐档调整 1–16 区块半径的视距。F3 切换调试信息，按住 F3 再按 G 切换区块边界调试线。

如需保存世界，在启动命令末尾添加 `-- --world ./target/demo-world`。正常关闭后，下次改用 `-- --open-world ./target/demo-world` 打开已保存的世界。目录缺少 `CURRENT` 时会直接报错。

可用独立服务端校验、备份、离线升级或选择历史代恢复存档。参见 [存档操作](doc/persistence.md#section-13-5)和[指定代恢复](doc/persistence.md#section-13-7)。

P4 测试使用 `--preset java-1.20.1`，提供经典超平坦、生存背包、本地命令行与逐 Tick 事件视图。[本地打包、启动与验收](doc/p4-compatibility.md)

### 运行无窗口演示

```sh
cargo run --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- --demo --seed 42
```

独立服务端演示移动、放置和破坏方块，完成后自动退出。

## 开发与贡献

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

- [工程文档索引](doc/README.md)和[文档标准](doc/documentation-standard.md)
- [桌面操作与验收](doc/p2-desktop-acceptance.md)
- [代码风格与测试约定](AGENTS.md)和[分支命名规范](doc/branch-naming.md)

项目自有源码采用 `GPL-3.0-or-later`。

## Lab1 实验记录

R3：验证工作区修改与差异查看。
