# P1 最小内核实施决策

依据[架构规格 §16](development.md#section-16)及本轮用户确认实施。RCF、DFL 和 TBD 编号沿用原规格。

## Cargo 工作区与依赖方向

用户确认采用单一 Virtual Workspace。完整目标约 16 个包。P1 使用五个 crate，内部优先按 Rust module 划分职责。新增 crate 应有独立依赖、可替换实现、测试或编译边界。

| Crate | 职责 | 工作区依赖 |
| --- | --- | --- |
| `rustcraft-world` | 坐标、Section、Chunk 数据与版本 | — |
| `rustcraft-mod-api` | 内建模组声明与依赖解析、冻结注册表、实体与组件契约 | world |
| `rustcraft-authority` | World/Dimension 启动、Tick 调度、权威提交、ECS 适配与只读快照 | world、mod-api |
| `rustcraft-vulkan` | Linux 窗口、Vulkan 设备和交换链、清屏呈现 | — |
| `rustcraft-host` | 本地消息、Tick 驱动、服务端与客户端入口 | authority、可选 vulkan |

持久化以 P2 存档需求为建包时机。服务端默认构建仅带权威内核，`client` feature 引入图形后端。TBD-004 的 P1 依赖边界已落地。

## ECS 与图形

- TBD-005 采用独立 `bevy_ecs`。每个维度拥有一个 ECS World。方块格位置用 Table，按需挂载的扩展组件用 SparseSet。世界分配稳定实体 ID，适配器维护 ID 和运行时 Handle 的映射。
- P1 调度按声明的阶段、依赖和读写集验证执行顺序。提交阶段按会话、操作序号排序，校验能力和对象版本，发布回执、变更摘要和只读快照。扩展组件以命名空间、版本和字段注册，经 RustCraft API 读写，存档编码留待持久化阶段。
- TBD-006 的 P1 路径采用 `ash`、`winit` 和 `ash-window`。首版设备要求是可呈现的图形队列以及支持 Transfer Destination 的交换链。当前图形验证入口使用 Vulkan 1.0，设备兼容矩阵与通用 RHI 接口仍待确定。

## P1 实现参数

50 ms 为本地 Tick 驱动间隔。Section 边长 16，方块坐标使用 `i64`。这些值用于 P1 实现和测试，TBD-008、009、017 的正式 Schema、性能与发布条件仍待确定。当前启动世界含一个空的原点 Chunk，种子进入权威配置和快照，地形生成始于 P2。
