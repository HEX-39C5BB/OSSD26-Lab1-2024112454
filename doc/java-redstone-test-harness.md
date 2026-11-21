# Java 1.20.1 红石参考测试设施能力审查

[文档索引](README.md) · [红石长期目标](java-redstone-goal.md) · [参考实验](java-redstone-verification.md) · [兼容矩阵](java-redstone-coverage.md)

## 目的和边界

本文审查 RustCraft 现有的 Java 1.20.1 红石参考测试设施，确定可复用能力和最小增量方案。当前约束：

- 不开发完整 `ControlledServerMain`。
- 不使用 `Thread.suspend()`。
- 不复制或重写 Minecraft DedicatedServer 的初始化流程。
- 不改变原版红石规则、事件顺序或可观察模拟语义。
- 测试基础设施不能成为红石兼容开发的主要阻碍。

## 现有设施

| 设施 | 已具备能力 | 当前限制 | 可复用部分 |
|---|---|---|---|
| `tools/java-redstone-reference/Reference.java` | GameTest 服务器内按 GT 推进、读取计划刻/Block Event、保存重开场景 | 无真实玩家连接；强制区块不能恢复 ticking | 反射字段、GT 推进、队列读取、夹具表达方式 |
| `tools/java-redstone-reference/BudAgent.java` | 附加到真实 DedicatedServer；读取 LevelTicks、Block Event、目标 Chunk 状态；每 GT 输出 JSONL | 只读；无暂停/单步；快照由 Server 线程任务采集；尚无 Phase/Sequence | Server 线程安全采集、Chunk 状态、JSONL 格式 |
| `tools/java-redstone-reference/NotificationAgent.java` | 只读 ASM 插桩 Level/BlockStateBase 通知入口；记录 GT、序号、类型、目标、来源、方向、深度、父 ID、线程；有界缓冲和 dropped | 依赖 JDK internal ASM；只覆盖已验证入口；异常路径退出配对仍有限 | 通知级 A/B 对照、NC/Shape 调用链 |
| `tools/run_notification_smoke.py` | 有/无通知 agent 的简单活塞 A/B 冒烟；状态屏障等待 resident 和队列状态 | 仅覆盖单活塞；尚未形成 BUD-008/009 正式场景 | 冒烟模板、后续多路径场景驱动 |
| `tools/run_bud008_matrix.py` | Datapack 函数同 GT 双来源；A/B/AB/BA 四场景、控制函数、通知与 Block Event 最终状态 | Java 基准；自动 first_divergence、重复/排除面/嵌套尚未覆盖 | BUD-008/009 多路径矩阵、同 GT 触发模板 |
| `crates/authority/tests/bud008_dual_source.rs` | Rust 同场景 A/B/AB/BA 回归：Check 来源顺序、同 GT、Shape 方向、单 Block Event、最终伸出 | 尚未自动读取 Java 原始轨迹做 first_divergence | Rust 语义回归、后续差分器基线 |
| `crates/authority/examples/bud008_trace.rs` | 导出 Rust 规范化语义事件：Check、Shape、Block Event、pending 和最终状态 | 仅 BUD-008/009 固定场景 | first_divergence 的 Rust 输入 |
| `tools/compare_bud008_trace.py` | 规范化 Java/Rust 轨迹，按来源/Shape/NC/相对 GT/顺序/队列/最终状态比较，报告 first_divergence；10 类自检 | 仅覆盖 BUD-008/009 当前四场景 | 语义差分、注入差异诊断 |
| `tools/run_dedicated_bud013.py` | 启动 DedicatedServer、协议 bot 加入、传送玩家、命令、最终状态和 trace | 命令用墙钟等待；场景逻辑与驱动混合；无正式场景清单 | 服务端启动、bot、命令、临时目录和日志管理 |
| `crates/authority/examples/bud013_trace.rs` | 导出 Rust 的队列和最终状态 | 仅按阶段输出，不是每 GT；场景硬编码 | 场景重建、`pending_java_block_ticks` / `pending_piston_events` |
| `tools/check_bud013_trace.py` | 校验 Java 队列保留/清除/Chunk 状态，比较 Rust 窗口摘要 | 不是逐 GT 序列；不验证同 GT 内顺序 | 场景摘要、状态断言、Java/Rust 比较框架 |
| `crates/authority/tests/java_redstone_reference.rs` | 解析既有 Reference JSONL，回放 P0 夹具 | 依赖独立 Java 参考 trace；部分用例只有游戏最终状态 | JSON 解析、夹具回放、差分断言 |
| `crates/authority/tests/java_flat.rs` | Rust 权威世界回归、Chunk active/inactive、灯/活塞 | 使用 `SetChunkActive`，不是实际卸载 | 确定性场景、队列断言 |

## 当前已确认的能力

- DedicatedServer + 协议 bot 能提供真实玩家 ticket。
- Java agent 已通过 `MinecraftServer.execute` 把队列快照提交到 Server 线程；后台线程只消费已完成的 JSON 行。
- Agent 已记录目标 Chunk 的 `ticking`、`ticket`、`resident` 状态。
- 已验证灯计划刻、普通活塞和跨 Section 活塞的 Block Event 在真实卸载期间保留、返回后清除。
- Rust example 能导出相同的队列窗口和最终状态，自动比较通过。
- BUD-007 的朝东 9 位置、四朝向关键位置、Chunk 接缝以及粘性活塞/发射器/投掷器组件矩阵已由 Reference 采集并由 Rust 差分回放；当前覆盖单一 QC 源和活动区块。
- 五段直线、9 段跨 Chunk、双电源、L 形转角、T 形双分支和跨 Section 的红石粉 + 活塞探针顺序均已与原版对齐；差分工具能定位首个偏差，修复后同一测试转绿。
- 五段粉链的 5 条 Block Event 已在真实区块卸载期间保留，返回后按 264→268 的相对顺序清除；计划刻的卸载保留仍由红石灯场景单独覆盖。
- 红石粉 `WirePower` 在每次邻居通知后立即递归分派新编辑，匹配 Java 的同步 `setBlock`；该改动同时保持发射器/投掷器测试通过。
- 发射器/投掷器的物品观察过滤 age>20 的旧掉落物，避免早先 fixture 留在附近的实体污染当前场景。
- 只读通知 agent 已接入 DedicatedServer，冒烟观测到 `neighbor_shape_changed` → 嵌套 `update_shape`，以及 `neighbor_changed` 行为入口；enter/exit、深度/父 ID 和 dropped=0 校验通过。
- 通知 agent 的 GT 改为在 Server 线程读取实际 `level.V()`；无关双 setblock 控制函数确认两条 setblock 可同 GT。
- BUD-008/009 双来源矩阵连续两次通过：A-only/B-only/AB/BA 均完成伸出，AB/BA 的初始来源通知同 GT，A/B 链串行，Block Event 只入队 1 个。
- Rust 同场景回归 `bud008_dual_source.rs` 已通过：来源 Check 顺序、同 GT、Shape 方向、单 Block Event 和最终伸出均与 Java 基准一致。
- `bud008_trace` + `compare_bud008_trace.py` 对 A/B/AB/BA 四场景均输出 `MATCH`；比较器自检覆盖一致、缺失、额外、顺序、重复、方向、GT、UNKNOWN、队列和最终状态十类差异。
- 这些结论是**选定场景的队列窗口验证**，不是完整逐 GT 事件序列验证；通知级 agent 目前完成单活塞冒烟和首个双来源 Java 基准。

## 通知级 Java agent 冒烟

`NotificationAgent.java` 通过 JDK internal ASM 只读插桩已验证入口，不修改参数、返回值、调度或世界状态。当前映射：

| 类 | 方法描述 | 语义 |
|---|---|---|
| `cmm` (Level) | `a(gu, cpn)` / `a(gu, cpn, ha)` / `a(gu, cpn, gu)` | 邻居更新分发及排除面候选 |
| `cmm` (Level) | `a(ha, dcb, gu, gu, int, int)` | Shape 通知分发 |
| `dca$a` (BlockStateBase) | `a(cmm, gu, cpn, gu, boolean)` | 方块行为入口 `neighborChanged` |
| `dca$a` (BlockStateBase) | `a(ha, dcb, cmn, gu, gu)` | 方块行为入口 `updateShape` |

`tools/run_notification_smoke.py` 在同一个简单活塞场景做 A/B：无插桩与有插桩各运行一次，比较最终状态和队列保留/清除，并校验通知轨迹。最近一次输出：

```text
PASS A/B behavior: final=(True, True, True) retained=True cleared=True
PASS notification smoke: events=134 kinds=['neighbor_changed', 'neighbor_shape_changed', 'update_shape'] max_depth=1 gt=25 dropped=0
CHUNK away={'ticking': False, 'ticket': False, 'resident': False}
CHUNK return={'ticking': False, 'ticket': True, 'resident': True}
```

结论和边界：

- 已观测到 `neighbor_shape_changed` 分发到嵌套 `update_shape`，以及 `neighbor_changed` 行为入口；`update_neighbors_at` 在该路径未命中，只作为辅助诊断，不据此判定原版缺失。
- 当前 `ticking` 字段不能直接等同于 Block Event 可处理状态；状态屏障使用 `resident` 和队列状态，事件执行通过返回后的活塞状态验证。
- 通知 agent 的 `gt` 改为在 Server 线程读取实际 `level.V()`。无红石干扰的双 setblock 控制函数在两次运行中均为同一 GT（样本 17、18）。
- `tools/run_bud008_matrix.py` 的 A-only/B-only/AB/BA 连续两次通过：AB/BA 的初始来源通知同 GT，A/B 链串行，Block Event 只入队 1 个；GT 40/43 等活塞头/运动通知已排除。
- 首个双来源 Java 基准已建立；Rust 差分、同源重复、排除面/方向和嵌套通知链仍待完成。

## 能力缺口

1. **场景调度**：Python 驱动仍以墙钟等待和硬编码步骤为主，没有统一的场景清单或相对 GT/Phase 操作表。
2. **生命周期边界**：通知冒烟已使用 `resident`/队列状态屏障；Dedicated BUD-013 主驱动仍以固定等待为主，且 `ticking`/`ticket` 的语义映射需要继续核对。
3. **事件阶段**：通知 agent 已有方法 enter/exit、深度和父 ID；入队、延后、执行、清除的 Phase/Sequence/Result 仍未合并成统一事件流。
4. **Chunk 语义分离**：Scheduled Tick 与 Block Event 的卸载、重载和重启语义尚未分别验证。
5. **Rust 轨迹**：Rust example 仍按阶段输出，不是每 GT；场景和 Java 不共享清单。
6. **差分工具**：比较器只比较队列窗口/最终状态，不比较完整事件顺序和因果关系。
7. **自检**：尚未故意注入延后、排序改变或事件丢失，验证差分工具能定位首次偏差。
8. **Tick Gate**：没有安全的暂停/单步能力；只作为最后手段。

## 最小增量计划

### 阶段 A：冻结现有 BUD-013 证据

状态：**已完成**。

- 保留现有灯/普通活塞/跨 Section 测试和队列窗口比较。
- 明确其范围是 selected queue-window，不声称完整 BUD-013。
- 不推翻已有黑盒证据。

### 阶段 B：Agent 安全边界和生命周期记录

状态：**部分完成**。

已完成：

- 快照通过 `MinecraftServer.execute` 在 Server 线程执行。
- Agent 记录目标 Chunk 的 `ticking`、`ticket`、`resident`。
- 后台线程只消费 JSON 行。
- 通知冒烟脚本使用 `resident` 和 Block Event 队列状态屏障，不依赖固定 sleep。
- 通知级 agent 完成 enter/exit、深度/父 ID、有界缓冲和 dropped 计数冒烟。

下一步：

- 把状态屏障推广到 Dedicated BUD-013 主驱动。
- 为每次状态变化记录 `requested_gt`、`observed_gt`、旧/新状态和原因。
- 不引入新的方块通知或权威状态。

### 阶段 C：场景清单和 Rust 逐 GT 导出

状态：**待实现**。

- 定义共享 JSON 场景文件：fixture、初始状态、相对 GT 操作、Phase、预期状态、Chunk 生命周期操作。
- Python 和 Rust 读取同一场景；Java agent 记录 Phase/GT 快照。
- Rust example 每 GT 导出 `pending_java_block_ticks`、`pending_piston_events` 和 Chunk 状态。
- 比较器先按相对 GT/Phase 对齐，再比较事件集合和顺序。

### 阶段 D：差分工具自检

状态：**待实现**。

- 在测试比较器中注入可控偏差：事件延后一 GT、顺序改变、事件丢失。
- 验证工具报告首次偏差的 fixture、GT、阶段、事件类型、位置和原因。
- 自检必须通过后才能把新场景标为 Verified Compatible。

### 阶段 E：可选辅助和 Tick Gate

状态：**最后手段**。

- 如需 Carpet 的 Tick Freeze/Step/SubTick，仅作为分析辅助；最终证据仍以未修改原版行为为准。需要时由用户手动提供 mod。
- 只有在阶段 B–D 无法覆盖关键差分时，才研究安全 Tick Gate；不直接复制 Main 初始化，不使用线程挂起。

## 需要修改的文件

| 文件 | 变更 |
|---|---|
| `tools/java-redstone-reference/BudAgent.java` | 保持 Server 线程安全；补充 Chunk 状态转换、Phase/Sequence（增量） |
| `tools/java-redstone-reference/NotificationAgent.java` | 只读 Level/BlockStateBase 通知插桩、深度/父 ID、有界缓冲 |
| `tools/run_notification_smoke.py` | 通知级 A/B 冒烟、状态屏障、轨迹校验 |
| `crates/authority/examples/bud008_trace.rs` | Rust 规范化语义轨迹导出 |
| `tools/compare_bud008_trace.py` | Java/Rust first_divergence 比较器和自检 |
| `tools/run_dedicated_bud013.py` | 场景清单、状态屏障、请求/观察 GT 记录 |
| `tools/check_bud013_trace.py` | 相对 GT/Phase 对齐、事件序列比较、自检失败注入 |
| `crates/authority/examples/bud013_trace.rs` | 每 GT 导出、读取共享场景 |
| `crates/authority/tests/java_flat.rs` | 对应事件生命周期回归 |
| `doc/java-redstone-verification.md` | 证据范围、限制和命令 |
| `doc/java-redstone-coverage.md` | BUD-013 状态更新 |
| `doc/java-redstone-goal.md` | 完成度和优先级更新 |
| `doc/java-redstone-test-harness.md` | 本文维护 |

## 测试目标和验收标准

### 阶段 B 验收

- Agent 所有队列读取都发生在 Server 线程任务中；后台线程不直接读 Minecraft 队列。
- 每个目标 Chunk 记录 `loaded/ticking/ticket` 状态和转换 GT。
- 状态屏障在 `resident=false/true` 或队列清除前不推进下一阶段。
- 现有灯/普通活塞/跨 Section 比较仍通过。

### 通知级冒烟验收

- 有插桩与无插桩的同一场景最终状态和队列保留/清除一致。
- 通知轨迹 enter/exit 配对，深度/父 ID 可重建调用链。
- 有界缓冲 `dropped=0`；出现丢失时不得作为顺序兼容性证据。
- 必需事件采用实际观测到的 dispatch + behavior 入口；未观测到的内部方法只作辅助诊断。
- `resident`/队列状态用于屏障，不把 `ticking` 字段直接等同于 Block Event 可处理状态。

### 阶段 C 验收

- Java 与 Rust 使用同一场景清单和相对 GT/Phase。
- 每个 GT 的事件集合、位置、类型、顺序可比较。
- 第一处偏差输出 fixture、GT、Phase、事件、预期/实际和原因。
- Scheduled Tick 与 Block Event 分别比较，不混为同一队列。

### 阶段 D 验收

- 注入事件延后一 GT 时，工具报告首个延后 GT。
- 注入顺序变化时，工具报告首个顺序偏差。
- 注入事件丢失时，工具报告缺失事件及位置。
- 工具自检失败时不能被误报为 Java/Rust 兼容。

### 生命周期状态验收

必须分别记录并区分：

1. Loaded + Ticking
2. Loaded + Non-Ticking
3. Unloaded
4. Reloaded
5. Server Restart 恢复

Scheduled Tick 与 Block Event 的保留、重载和重启语义分别验证；不得假定两者相同。

## 当前优先级

1. BUD-007 和方块更新链/事件顺序。
2. BUD-013 的 Chunk 生命周期、Scheduled Tick 与 Block Event 语义。
3. Agent 确定性场景调度和逐 GT/Phase 追踪。
4. Java/Rust 自动轨迹差分。
5. 必要时研究安全 Tick Gate。
6. 最后才考虑完整 ControlledServerMain。

## 当前结论

- 现有设施可以复用，不需要重新实现 Minecraft 主循环。
- 最低风险增量是 Server 线程安全快照、Chunk 状态记录和状态屏障。
- 完整逐 GT 差分应建立在共享场景清单和可信状态边界上，而不是强制同步两端绝对 GT。
- 在阶段 C/D 验收前，BUD-013 仍标为 **部分完成（选定队列窗口）**。
