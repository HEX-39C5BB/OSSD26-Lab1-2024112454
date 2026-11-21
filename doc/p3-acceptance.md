# P3 验收清单

[文档索引](README.md)

本次复验记录于 2026-11-19。构建使用 Linux x86-64 和 Rust 1.98.1。存档中断用例使用 Btrfs 上的临时目录。窗口与音频证据注明各自的 Gentoo 实机范围。

P3 本地收口门槛已通过。离线任务管理与保存恢复重放均通过，Intel 核显五档窗口复验通过，NVIDIA 独显五档补测完成。

## 本地复验

从仓库根目录运行以下命令。格式、文档和 Clippy 检查须以零退出，全功能工作区测试须无失败。独立服务端演示须输出 `event=p2_demo_complete`。

```sh
cargo fmt --all -- --check &&
python3 tools/check_docs.py &&
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings &&
cargo test --locked --workspace --all-features &&
cargo run --release --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- --demo --seed 42
```

| 项目 | 结果与证据 |
|---|---|
| 工作区与独立服务端 | 95 项测试通过，格式、Clippy 和文档检查通过。存档中断子进程入口由两个父测试启动，常规测试列表将它标为 `ignored`。客户端与独立服务端 Release 构建通过。 |
| 存档与进程中断 | 在新代目录同步后、更新 `CURRENT` 前终止写入子进程，重开读旧代。`CURRENT` 持久化后终止，重开读新代。两种情况继续保存均发布下一代。参见 [进程中断测试](../crates/persistence/src/crash_tests.rs)与[恢复契约](persistence.md#section-13-4)。 |
| 模拟与确定性 | 保存恢复后的 285 个 Tick 与连续运行一致。离线排队和取消命令在原 Tick 发布新代，重开后的任务执行与连续运行一致。专项测试覆盖优先顺序、替换冲突、停用和取消。参见 [专项验收命令](development.md#section-15-1)与 [延迟任务命令](simulation.md#section-4-5)。 |
| 五档无窗口区块生命周期 | 种子 42、每档 64 个采样 Tick。半径 1、4、8、12、16 的活动区块峰值依次为 9、81、289、625、1089。参见 [基准命令](performance.md#section-14-3)。 |
| Gentoo 历史流送门槛 | 复核 `target/streaming-baseline.6cTrUc` 的六份原始日志，36 次跨界全部补齐。半径 2 最大 113.953 ms，上限 135 ms。半径 4 最大 209.559 ms，上限 235 ms。参见 [门槛与复核命令](performance.md#streaming-window)。 |
| Gentoo Intel 五档窗口 | 半径 1、4、8、12、16 均为实际 1920×1080，30 次跨界全部补齐。半径 4 最大 207.956 ms，通过 235 ms 门槛。参见 [实测记录](performance.md#gentoo-expanded-results)。 |
| Gentoo NVIDIA 五档补测 | 确认使用 RTX 4060 Laptop GPU，五档同为实际 1920×1080，30 次跨界全部补齐。参见 [实测记录](performance.md#gentoo-expanded-results)。 |

上一组命令构建独立服务端后，在同一目录复验五档活动区块峰值。

```sh
sh -c 'for radius in 1 4 8 12 16; do target/release/rustcraft-server --benchmark --seed 42 --ticks 64 --radius "$radius" || exit; done'
```

## GitHub 与 Gentoo 剩余验收

当前阶段只推进本地开发与验收。GitHub 执行记录作为后续证据，远端 GPU 首帧延期至云服务器确定后。参见 [TBD-024](decisions.md#section-19)。

| 项目 | 已有证据 | 待补证据 |
|---|---|---|
| GitHub 基础 Linux CI | [工作流](../.github/workflows/linux-ci.yml)包含在 `feat/p3-local-stability` 的 `553a01f` 检查点。 | 归档包含该工作流的 PR 或 `main` 提交的首次成功运行链接和提交 SHA。参见 [GitHub Actions](https://github.com/HEX-39C5BB/RustCraft/actions)。 |
| 专用 GPU Runner（延期） | `Gentoo-Impact` 已注册。2026-11-19 的作业在步骤执行前取消，Runner 日志记录了 GitHub Broker 请求超时。参见 [取消的运行](https://github.com/HEX-39C5BB/RustCraft/actions/runs/44495404779)。 | 云服务器确定后恢复验收。归档 `Vulkan first frame on GPU` 作业的运行链接、提交 SHA、实际 GPU 和 `event=vulkan_terrain_first_frame_presented`。参见 [Runner 要求](development.md#section-15-4)。 |
| Gentoo 桌面补测 | 用户已确认早期 Wayland 键鼠、原生 PipeWire 音效和首版 F3、F3+G。 | 归档新版 3×3 黄线与上下各 3 子区块横线、双击取消飞行的复验结果。X11 暂不支持。参见 [桌面验收步骤](p2-desktop-acceptance.md)。 |

归档实机日志时记录构建提交、运行环境、设备和参数。扩展视距的性能阈值根据新实测确定，已有半径 2 和 4 的门槛继续用于对应历史场景。参见 [性能验收](performance.md#section-14-3)。
