# P2 首个里程碑

本轮采用无窗口验收，覆盖平坦原创世界、第一人称移动与碰撞、权威方块放置与破坏。客户端 Vulkan 首帧仍按 [P1 验收记录](p1-acceptance.md)单独验证。

## 运行演示

```sh
cargo run --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- --seed 42 --demo
```

演示通过进程内玩家会话发送移动、瞄准、放置和破坏指令。它等待重力使玩家落地，验证玩家前移，再检查同一格的方块被放置并移除。成功时输出 `player_moved`、`block_placed`、`block_broken` 和 `p2_demo_complete`，随后正常关闭世界。

## 实现边界

- `WorldGenerator` 生成平坦分层地形，启动时激活以原点为中心的 3×3 区块。地表为地衣岩，下面为壤土与玄武岩。同一种子重现壤土层中的玄武岩分布。
- 本地玩家由所在维度的 ECS World 持有。输入提交后，固定 Tick 执行重力、跳跃和逐轴方块碰撞，再发布玩家位置与选中目标。玩家实体使用 P2 临时 `f64` 坐标，正式坐标编码沿用 TBD-008。
- 方块操作使用权威玩家位置和朝向进行 5 格射线检测。破坏验证命中区块版本，放置同时验证相邻区块版本与玩家碰撞体。当前默认放置材料为玄武岩。

## 验收

```sh
cargo test --locked --workspace --all-features
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
```

测试覆盖种子复现、正负区块边界、落地、跳跃、撞墙、进出会话、遮挡、距离、跨区块放置、过期目标及重复操作。下一里程碑接入库存与存档，窗口中的键鼠操作与地形显示随图形路径接入。
