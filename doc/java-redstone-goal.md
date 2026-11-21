# Java 1.20.1 红石长期目标与达成情况

[文档索引](README.md) · [行为规格](java-redstone-model.md) · [兼容矩阵](java-redstone-coverage.md) · [参考实验](java-redstone-verification.md) · [测试设施审查](java-redstone-test-harness.md)

<a name="redstone-goal-status-labels"></a>
## 状态口径与文档用途

本文登记 Java 1.20.1 红石系统的长期开发目标、当前达成情况、差距和实施顺序。具体行为语义见[红石行为规格](java-redstone-model.md)，逐元件状态见[兼容矩阵](java-redstone-coverage.md)，原版证据见[参考实验](java-redstone-verification.md)。

- **Implemented**：RustCraft 已具备该项行为；标为“原型”时注明限制。
- **Tested**：RustCraft 有记录的回归覆盖；测试只证明所列范围。
- **Verified Compatible**：有指定 Java 1.20.1 装置的原版证据和同装置差分结果。数据报告、单侧实验或单侧测试都不足以获得此状态。
- **TBD**：行为细节、实现或证据尚未补齐。
- “部分”表示只在列出的装置、方向、阶段或边界内成立，不表示全局兼容。

本文不把“代码已存在”“测试已通过”“原版差分已通过”混为一项。未完成全部适用场景前，不对外声称完整 Java 1.20.1 红石兼容。

<a name="redstone-goal-baseline"></a>
## 目标与固定基线

**EXPLICIT** 兼容基线固定为 Minecraft Java Edition 1.20.1，规则身份为 `java/1.20.1`。已有超平坦存档分别绑定 `rustcraft:java_1_20_1_flat@1`、`@2` 和 `@3`；`@2`/`@3` 使用时序版本 2 和 Java 内容包 0.2.0，`@3` 额外按原版重开实验区分临时 Block Event 与持久计划刻。旧入口与存档不自动迁移。[规则组合](simulation.md#section-5) · [TBD-001](decisions.md#section-19)

目标要求：

1. 常规红石电路在 Java 1.20.1 规则下正确运行。
2. 准确复现 GT、RT、计划刻、Block Event 和 Tick 内阶段时序。
3. 方向性普通/直接输出、传导、强弱充能、组件激活与实际状态彼此分离。
4. 区分 NC、Shape Update、Comparator Update、计划刻和 Block Event 的产生、传播、入队、执行与提交。
5. 复现 QC、BUD、短脉冲、活塞吐块及依赖更新顺序的技术行为。
6. 用原版参考实验、回归测试和差分测试记录证据；无法观测的内部细节标为 TBD。
7. 保持确定性、权威提交、版本化规则和可替换实现；优化不得改变可观察语义。

非目标与边界：

- Bedrock、Redstone Experiments、其他 Java 版本和 Strict 规则集不自动成为 1.20.1 默认行为。
- 不为了运行特定红石机器编写硬编码补丁；特殊机制必须由底层规则自然组合产生。
- 不把“供电查询变化”自动转化为“向所有依赖方广播更新”。
- 不引入会改变原版行为的权威 `is_budded` 开关或周期性全图修正扫描。
- 不因缓存、异步完成顺序、渲染、存储或网络同步产生额外通知或动作。

核心原则：**红石不是只按电路拓扑传播信号的系统，而是由时间、供电、通知、事件顺序和方块行为共同组成的离散事件模拟系统。不要专门模拟 BUD 的结果，要正确模拟使 BUD 自然发生的底层机制。**

<a name="redstone-goal-capabilities"></a>
## 架构能力与当前达成情况

| 目标能力 | 当前实现位置 | 状态 | 主要缺口 |
|---|---|---|---|
| Simulation Clock | `time.rs`、`tick_clock.rs`、`Server::step_local` | **Implemented** / **Tested** | 实际墙钟追赶与权威 GT 已分离；红石专项性能样本未建立 |
| Redstone Timing Semantics | `java_block_ticks.rs`、`schedule.rs` 与各元件计划刻 | **Implemented**（P0）/ **Verified Compatible**（选定装置） | 完整 Tick 内阶段、同刻重入和链式边界 TBD |
| Power Query | `java_signal.rs`、`java_wire.rs`、`power_output` | **Implemented** / **Verified Compatible**（选定查询） | 强/弱充能对所有组件和方块类型的完整规则仍待补齐 |
| Activation Rules | `piston_power`、各元件激活入口 | **Implemented**（P0）/ **Tested** | P1/P2 元件与更多方向、遮挡和边界条件 TBD |
| Block Update Dispatcher | `java_redstone.rs` 的 NC、Shape、Comparator 分派 | **Implemented**（原型）/ **Tested**（部分）/ **Verified Compatible**（选定双来源、重复、六向和排除面样本） | 通知标志扩展、一般递归更新、深度上限与嵌套链仍 TBD |
| Scheduled Tick Scheduler | `JavaBlockTicks`、`ScheduledBlockTick` | **Implemented** / **Tested** / **Verified Compatible**（`ORDER-CHUNK`） | 失活区块恢复、同刻重入和全部元件的重开语义待验 |
| Block Event Scheduler | 活塞事件队列、`PistonTrace`、运动阶段 | **Implemented**（P0 活塞）/ **Verified Compatible**（选定装置） | 复杂负载、多活塞、同刻事件和在线区块生命周期待验 |
| Redstone Component Behavior | `java_*.rs` 元件模块与 `redstone.rs` 状态注册 | **Implemented**（P0 全部）；P1 未开始；P2 部分 | P1 全部 TBD；P2 仅粘性活塞吐块等局部 |
| Redstone Trace / Debugger | `java_bud.rs`、`--inspect-bud`、`--trace-bud`、NDJSON 事件 | **Implemented**（单点）/ **Tested**（部分） | 多点追踪、完整跨组件因果、全局状态摘要和客户端 UI TBD |
| Compatibility Test Harness | `tools/run_java_redstone_reference.py`、ignored 差分测试、通知级 A/B 冒烟、BUD-008/009 双来源矩阵与 first_divergence 比较器 | **Implemented** / **Tested**（已采样本） | 同源重复已加离线 GameTest 与 Rust 回归；在线矩阵尚未执行，P1/P2、全部方向、在线区块和特殊机制未覆盖 |

空间邻接、连接拓扑和更新因果在实现中分开：`Face`/`ChunkPos` 提供几何寻址，红石粉端口和供电查询只产生候选结果，实际通知由 `java_redstone.rs` 的事件分派决定。电源拓扑不能自行决定更新接收者；派生缓存失效也不进入通知队列。

<a name="redstone-goal-time"></a>
## 时间、调度与阶段

| 语义 | 当前状态 | 证据或限制 |
|---|---|---|
| 20 GT/s，1 GT 名义 50 ms | **Implemented** | `GAME_TICKS_PER_SECOND = 20`、`GAME_TICK_INTERVAL = 50 ms` |
| 1 RT = 2 GT，中继器 1–4 档 | **Implemented** / **Verified Compatible** | 四档边沿 2/4/6/8 GT，奇偶 GT 均有采样 |
| 一个 GT 内多次有序更新 | **Implemented** | Commit 阶段按请求排序，组件在同一阶段内提交多条编辑 |
| Immediate Update / NC / Shape | **Implemented**（原型）/ **Verified Compatible**（选定排除面矩阵） | 已区分 NC 与 Shape Update；六向排除顺序通过样本；一般递归深度与嵌套链 TBD |
| Scheduled Block Tick | **Implemented** / **Tested** | 类型化队列、到期 GT、优先级、`sub_tick_order`、同位置同类型去重 |
| Scheduled Tick Priority | **Implemented** / **Verified Compatible**（样本） | `ORDER-CHUNK` 覆盖跨 Chunk、到期和优先级；完整组合 TBD |
| Block Event | **Implemented**（活塞）/ **Verified Compatible**（样本） | 执行时复查供电、目标和运动状态；重复身份和阶段内新事件待验 |
| 运动完成与中间态 | **Implemented**（P0 活塞）/ **Verified Compatible**（样本） | 两 GT 运动占位、持久化进度；复杂负载和碰撞待验 |
| 组件在不同阶段收到输入的差异 | **部分** | 已有 `TIM-PHASE` 设计和部分差分；完整阶段矩阵 TBD |
| 1 GT 脉冲、0-tick、短脉冲 | **部分** | 粘性活塞短脉冲吐块有证据；0-tick 和更广泛短脉冲 TBD |

RT 是延迟计量单位，不是独立于 GT 的主循环；实现不强制所有组件每 2 GT 才能执行一次，同一 GT 内仍可发生多次有序更新。调度顺序也不是简单“当前 Tick + 固定延迟”：计划刻保留到期 GT、优先级和刻内序号；Block Event 保留独立身份并在执行阶段重新检查。`/tick` 和 F3+R 是 RustCraft 调试控制面，不冒充原版命令表。

<a name="redstone-goal-power"></a>
## 供电、激活与方块更新

实现区分了查询与提交：

- `power_output` 返回 `normal`、`direct` 和 `conducted`。普通输出对应通常的弱信号路径，直接输出和经导体传导用于表达强充能及特定组件读取；`PowerContext` 区分红石粉外部输入和一般查询。
- `piston_power` 返回直接输入、准连接输入、未供电或不可用；读取本身不发送通知。
- 实际 BlockState、计划刻、Block Event、运动状态和比较器输出分别存储；激活条件不等于实际状态，收到更新也不等于已经执行动作。
- 红石粉的形状、端口和电平分开记录；形状更新、电平更新和邻居通知各有入口。
- 缺失区块与空气不同：查询返回不可用，不能制造 0 供电或默认激活。

| 更新类别 | 当前状态 | 说明 |
|---|---|---|
| Neighbor Changed（NC） | **Implemented**（原型）/ **Tested**（选定顺序和排除面） | 已覆盖已提交编辑、指定通知、六方向及六种单面排除；全局递归链与深度规则 TBD |
| Shape Update | **Implemented**（原型） | 支撑、连接、观察者和活塞形状入口独立；标志组合待验 |
| Comparator Update | **Implemented**（部分） | 可穿过一个导体通知比较器；容器和展示框输入未完 |
| Scheduled Block Tick | **Implemented** / **Tested** | 独立于通用持久任务 |
| Block Event | **Implemented**（活塞） | 独立于计划刻；执行时重新检查 |
| Commit / Publish | **Implemented** | 权威写入原子提交、revision 和差异发布；派生失效不入通知队列 |

当前实现仍不能宣称完整复现 Java 1.20.1 的更新深度、去重和同 Tick 顺序。不得为了性能优化随意排序、去重、合并或丢弃会影响原版行为的更新。通知范围、重复合法更新、链式上限、区块失活恢复和跨 Chunk 边界继续按矩阵中的 TBD 推进。

<a name="redstone-goal-bud"></a>
## QC/BUD 达成情况

活塞、粘性活塞、发射器和投掷器已有 QC 查询和只读诊断。核心约束是：供电条件变化不等于组件收到更新；组件可以在满足条件时保持原状态，直到收到符合规则的 NC/Shape 等更新。实现没有周期性扫描并自动修正状态。

只读诊断覆盖：

- 方块类型、位置、朝向和当前 BlockState。
- 直接/准连接激活来源及查询结果。
- 最近有效检查、待办计划刻、Block Event、运动状态和阻挡。
- GT、Tick 内阶段和事件序号；最近 256 条单点记录。
- `missing_update`、`pending`、`moving`、`blocked`、`history_insufficient` 等状态分类。
- 诊断不把待执行动作、合法运动或阻挡直接判为 BUD；只有同时满足历史证据和状态失配条件才报告疑似缺少通知。
- 诊断不写方块、不补通知、不修改任务、不新增持久化标志；结果在相同输入和规则版本下应可重复，且不因渲染、缓存、存储或网络同步改变。

| 用例 | 当前状态 | 说明 |
|---|---|---|
| BUD-001 直接供电正常激活 | **Verified Compatible**（选定装置） | 朝东普通活塞及直接源；其他方向/组件部分覆盖 |
| BUD-002 QC 供电且收到有效 NC | **Verified Compatible**（选定装置） | 记录立即状态、入队和动作 GT |
| BUD-003 QC 供电但无有效 NC | **Verified Compatible**（选定装置） | 至少 5 GT 保持，配阳性对照 |
| BUD-004 保持 QC 后有效 NC | **Verified Compatible**（选定装置） | 伸出、移动占位和完成顺序 |
| BUD-005 撤 QC 无有效 NC | **Verified Compatible**（选定装置） | 验证保持伸出 |
| BUD-006 撤 QC 后有效 NC | **Verified Compatible**（选定装置） | 收回、运动结束和原位状态 |
| BUD-007 无关方块变化与传播范围 | **Verified Compatible（选定矩阵 + 扩展粉链）** | 朝东 9 位置、四朝向关键位置、Chunk 接缝、组件矩阵、五段直线、9 段跨 Chunk、双电源、L 形转角、T 形双分支和跨 Section 粉链均通过 Rust 差分；更长/更多分叉/卸载链 TBD |
| BUD-008 Shape/NC/缓存失效分离 | **Verified Compatible（选定双来源、重复、六向和排除面矩阵）** | 单 QC 活塞 A-only/B-only/AB/BA 的 Java 基准连续两次通过；Rust 同场景回归和规范化 first_divergence 比较全部 MATCH。同源玻璃→空气→玻璃的 Java/Rust 样本均有 3 次来源通知、单项队列和下一 GT 伸出，Rust 还记录 1 次入队与 2 次 Duplicate。六方向 Shape-only 不激活、随后 NC 激活；六种排除面各派发剩余五邻居并按 Java 顺序，Rust 同步回归通过。同源在线 first_divergence、全部通知标志和嵌套链仍 TBD |
| BUD-009 同 GT 多次改变激活条件 | **Verified Compatible（选定同 GT 双来源顺序）** | 同一 GT 内 A→B 与 B→A 顺序已复现两次；Rust 对应顺序、同 GT、单事件和最终状态一致；完整多事件因果组合 TBD |
| BUD-010 待执行 Block Event 时撤源 | **Verified Compatible**（选定装置） | 执行阶段重新检查 |
| BUD-011 方向、坐标、旋转和镜像 | **部分** | 普通活塞六向和部分边界；全组件旋转/镜像 TBD |
| BUD-012 保存、退出、重新加载 | **Verified Compatible**（选定活塞/容器） | 独立 JVM 重开；在线区块卸载/重载 TBD |
| BUD-013 跨 Chunk/Section 边界 | **部分完成（选定队列窗口 + 五段粉链 + 双区块停用）** | DedicatedServer + 协议 bot + 只读 agent 与 Rust 逐 GT 队列导出已自动比较：灯计划刻、普通活塞、跨 Section 活塞和五段粉链的 5 条 Block Event 在卸载期间保留、返回后清除且最终状态一致；粉链事件相对顺序稳定为 264→268。新增 Java GameTest/Rust 回放证明两个常驻区块中只停用一个时，活动区块事件继续执行、停用区块事件保留并在重启用后完成。该离线用例明确保持两个 Chunk resident=true，不代表真实卸载。完整逐 GT 事件序列、真正部分卸载、粘性活塞/发射器/投掷器和全部方向仍 TBD |
| BUD-014 缓存开关等价 | **TBD** | 尚无电力图缓存可切换 |
| BUD-015 异步预计算等价 | **TBD** | 尚无异步预计算实现 |

已记录的原版差分包括普通活塞 90 项状态和 30 项队列、红石灯 61 项状态和 42 项队列、V3 重开 356 项活塞状态、152 项事件和 51 项进度，以及发射器/投掷器 34 项容器观察。BUD-008/009 另行扩展到粘性活塞、发射器、投掷器：粘性活塞 11 项状态和 7 项队列，发射器和投掷器各 14 项状态和 14 项队列。BUD-007 的 9 位置更新范围矩阵已采集并由 Rust 回放，结果见[验证记录](java-redstone-verification.md#bud007-evidence)。DedicatedServer + 协议 bot + 只读 agent 已导出在线卸载期间的灯计划刻、普通活塞和跨 Section 活塞 Block Event；Rust 逐 GT 队列导出与 Java agent 的队列窗口自动比较通过。完整逐 GT 事件序列仍未比较。它们只证明对应装置，不外推到全部 BUD 场景。[组件级 BUD 证据](java-redstone-verification.md#component-bud-evidence) · [DedicatedServer 探测](java-redstone-verification.md#dedicated-bud013) · [活塞证据](java-redstone-verification.md#piston-evidence) · [重开证据](java-redstone-verification.md#qc-restart-evidence) · [诊断](java-redstone-verification.md#bud-inspection)

<a name="redstone-goal-components"></a>
## 元件范围与兼容矩阵

- **P0 核心元件**：红石粉、拉杆、按钮/压力板、红石块、红石火把、中继器、比较器、观察者、普通活塞、粘性活塞、红石灯、发射器/投掷器。全部有实现和回归测试；均有至少一项选定原版差分，但完整方向、嵌套通知、特殊负载和在线区块生命周期仍为 TBD。
- **P1 扩展元件**：漏斗、箱子/陷阱箱、讲台/展示框、音符盒、目标方块、铁轨、阳光探测器、绊线、幽匿感测体等目前均未实现，状态为 **TBD**。
- **P2 技术红石**：粘性活塞吐块已有原版差异和重开证据；普通活塞简单直线推动为原型；Slime/Honey、移动碰撞、0-tick、活塞连锁、TNT 复制、刷线/刷沙和完整更新上限仍为 **TBD**。

逐项状态、验证轴和证据链接以[兼容矩阵](java-redstone-coverage.md)为准。本文不复制元件细节，避免两份状态表漂移。

<a name="redstone-goal-trace"></a>
## 追踪、差分测试与证据

已有能力：

- 单点 BUD 追踪使用 NDJSON 记录 GT、阶段、顺序、父事件、来源/目标、方向、前后状态、供电/激活结果、计划刻和可观察状态摘要；通用事件格式已在[参考实验](java-redstone-verification.md#trace-format)中定义，但尚未覆盖全局事件流。
- `--inspect-bud WORLD X Y Z` 读取检查点并输出只读诊断；`--trace-bud WORLD X Y Z TICKS` 在内存中推进并从同一代重放。
- 原版参考实验使用 Java 1.20.1 客户端、Java 17、GameTest 世界和独立 JVM；轨迹保存在本机 `target/java-redstone-reference.*/`。
- 差分测试在提供 `RUSTCRAFT_JAVA_REDSTONE_REFERENCE` 时运行，覆盖计划刻、供电、活塞、灯、中继器、比较器、观察者、火把、红石粉、控制件和发射器。

未完成：

- 世界全局、多目标和完整跨组件因果追踪。
- 客户端可视化调试 UI。
- 所有方向、镜像、加载状态和 P1/P2 机制的原版轨迹。
- 原版内部阶段不可观测时的推断声明；这些位置必须保持 TBD。
- 与原版行为分布兼容的非确定性场景分类。

比较层级当前进度：最终 BlockState 和选定逐 GT 序列已有多组证据；关键事件因果只在单点和已记录队列范围成立；组件运动中间态有部分证据；长时间运行的技术机器尚未系统覆盖。

<a name="redstone-goal-deliverables"></a>
## 交付物与最终验收对照

### 第一阶段交付物

| 交付物 | 状态 |
|---|---|
| Java 1.20.1 红石完整行为规格 | **部分**：P0 精确度较高；P1/P2 大量 TBD |
| GT、RT、计划刻和 Block Event 时序设计 | **Implemented** / **部分验证** |
| 充能、传播、激活和状态语义模型 | **Implemented**（P0）/ **部分验证** |
| 方块更新类别、范围和顺序说明 | **部分**：类别已分离，完整顺序/上限 TBD |
| QC/BUD 成因、检测流程和状态诊断 | **Implemented** / **部分验证** |
| 元件与特殊机制兼容矩阵 | **已建立**，大量 TBD |
| 对照测试、装置说明和事件追踪格式 | **已建立** / **部分覆盖** |
| 现有架构与目标行为差距及实施计划 | **本文与行为规格 §当前差距** |

### 实现阶段交付物

| 交付物 | 状态 |
|---|---|
| 最小红石事件与调度参考实现 | **Implemented** |
| P0 核心元件及自动化测试 | **Implemented** / **Tested**，边界未全覆盖 |
| QC/BUD 专项测试及复现 | **部分**：BUD-001–013 选定场景；014/015 TBD |
| P1/P2 逐步实现 | **未完成**：P1 未开始；P2 仅局部 |
| 持续回归与性能测试 | 回归测试已有；红石专项性能门禁未建立 |
| 逐项 Implemented/Tested/Verified Compatible 标记 | 已按矩阵使用，仍需持续补齐证据 |

### 最终验收标准

| 标准 | 状态 |
|---|---|
| 1. 常规 Java 1.20.1 红石电路正确运行 | **部分**：P0 电路和选定装置 |
| 2. 红石刻、GT 和方块事件时序准确 | **部分验证** |
| 3. 强弱充能、方向性信号和激活判定准确 | **部分验证** |
| 4. 方块更新种类和关键执行顺序准确 | **部分** |
| 5. QC/BUD 在无更新与有效更新下正确复现 | **部分验证** |
| 6. 活塞、观察者、中继器、比较器和粉末复杂交互 | **部分验证** |
| 7. 短脉冲、0-tick 和技术红石有独立证据 | **部分/TBD** |
| 8. 缓存或并行优化不破坏版本规则 | 尚无优化实现；需在启用前通过等价测试 |
| 9. 能解释失败的 Tick、事件、供电和因果 | **部分**：单点轨迹与诊断 |
| 10. 1.20.1 不与其他版本混合 | **Implemented**：规则身份和预设隔离 |

**总体结论：未达到最终验收。** 当前完成的是 P0 核心、类型化调度、方向性查询、QC/BUD 基础复现和选定原版差分；P1 未开始，P2 只完成局部，完整更新链、全量 BUD 矩阵、全局追踪和优化门禁仍未完成。因此不能对外宣称“完整 Java 1.20.1 红石兼容”。

<a name="redstone-goal-gaps"></a>
## 差距与实施顺序

1. **补齐 P0 边界**：完成各方向、镜像、替换、支撑、嵌套通知、同刻批次、区块失活/恢复和特殊负载的原版对照。
2. **完成更新语义**：实测 NC/Shape/Comparator 的标志、排除面、递归深度、重复通知、链式上限和同 Tick 顺序；不得用统一 BlockUpdate 替代。
3. **闭合 BUD 矩阵**：对普通/粘性活塞、发射器、投掷器跑完 BUD-001–013 的可适用变体；在引入缓存和异步前先定义并跑 BUD-014/015。
4. **扩展追踪**：从单点 BUD 轨迹扩展到多目标、跨组件因果、全局摘要和客户端可视化；失败报告必须能定位 GT、阶段、供电和更新差异。
5. **实现 P1**：按漏斗/容器、音符盒/目标、铁轨、光照/绊线、幽匿感测体的依赖顺序推进，每项单独建立证据。
6. **实现 P2**：先完成 Slime/Honey、移动许可、碰撞和多活塞，再处理 0-tick、TNT、刷线和历史机制；所有特殊机器必须由底层规则组合产生。
7. **优化门禁**：电力图缓存、局部查询缓存、脏区域、异步预计算和并行推测只能在参考实现、差分测试和检查点重放等价后启用。
8. **性能与回归**：建立 Tick、事件队列、区块生命周期、内存和长时间机器运行基准；性能通过不能替代逻辑兼容证据。

输入、装置、坐标、规则版本、观察阶段和预期结果必须随测试用例保存；无法获得原版内部轨迹时记录观测限制，不得把 RustCraft 自身轨迹当作原版真值。

<a name="redstone-goal-evidence"></a>
## 证据与维护入口

- 行为语义：[java-redstone-model.md](java-redstone-model.md)
- 逐项状态：[java-redstone-coverage.md](java-redstone-coverage.md)
- 原版实验与追踪格式：[java-redstone-verification.md](java-redstone-verification.md)
- 运行和本地验收：[p4-compatibility.md](p4-compatibility.md)
- 决策状态：[TBD-001](decisions.md#section-19)
- 实现入口：`crates/authority/src/java_redstone.rs`、`java_signal.rs`、`java_block_ticks.rs`、`java_bud.rs` 及 `java_*.rs` 元件模块
- 参考工具：`tools/run_java_redstone_reference.py`
- 差分测试：`crates/authority/tests/java_redstone_reference.rs`
- 诊断测试：`crates/authority/tests/java_bud.rs`、`crates/host/tests/bud_commands.rs`

验证时先运行常规回归，再按需提供原版参考轨迹运行 ignored 差分：

```sh
cargo test --offline --locked -p rustcraft-authority --test java_bud
cargo test --offline --locked -p rustcraft-authority --test java_flat
cargo test --offline --locked -p rustcraft-host --test bud_commands

RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference -- --ignored
```

维护规则：新增元件或更新规则时，同时更新行为规格、兼容矩阵、参考实验和本文的达成状态；只有拿到指定版本证据和同装置差分后才提升为 **Verified Compatible**，否则保留 **TBD** 或“部分”。
