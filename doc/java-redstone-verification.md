# Java 1.20.1 红石参考实验

[文档索引](README.md) · [长期目标与达成情况](java-redstone-goal.md) · [行为和调度规格](java-redstone-model.md) · [兼容矩阵](java-redstone-coverage.md) · [测试设施审查](java-redstone-test-harness.md)

## 参考运行

基准固定在 Java Edition 1.20.1。使用已由 Prism Launcher 安装的 1.20.1 客户端、对应库及 Java 17，从仓库根目录运行。客户端 JAR 的 SHA-256 和内置版本信息须匹配，数据版本为 3465。脚本在 `target/java-redstone-reference.*/` 为每次运行建立独立世界，保存 `reference.log` 和 `reference.jsonl`。世界、依赖库、原版资源和日志留在本机。

```sh
python3 tools/run_java_redstone_reference.py \
  --prism-root "$HOME/.local/share/PrismLauncher" \
  --java /opt/openjdk-bin-17/bin/java
```

参考脚本启动原版客户端 JAR 内置的 GameTest 世界并调用 `ServerLevel.tick`。输入在相邻两次 GT 之间通过原版命令、邻居通知、形状更新或放置回调执行。轨迹保存世界种子、绝对坐标、操作和样本，包含输入返回、每次维度 GT 完成和两个方块事件队列的观察点。脚本使用未修改的原版二进制，反射读取队列及供电判定。装置位于活动区块。脚本顺序启动 `fixtures`、`qc-save`、`qc-load` 三个独立 JVM。后两项验证实际保存退出及进程重开，全部阶段完成才写入成功标记。`--timeout` 分别限制每个进程。完整手持交互、网络和在线区块生命周期另设试验。[1.20.1 ServerLevel 映射](https://mappings.dev/1.20.1/net/minecraft/server/level/ServerLevel.html)

数据报告可核对组件属性和默认状态。本地运行 `net.minecraft.data.Main --reports` 的 `reports/blocks.json` SHA-256 为 `f10d5da83ec8cd995d4f484b788c5b1467a490728e687c629a681450e3253017`。数据报告不含事件顺序和供电因果。GameTest 采样也只暴露已记录观察点，无法凭两个样本间的变化断言所有内部通知的顺序或次数。原版内部的阶段、优先级和因果缺少证据时标为 **TBD**。具体证据随 [矩阵](java-redstone-coverage.md) 维护。

## 装置约定

基础活塞朝东，基座为 `P = (4,6,6)`，前方 `P + east` 留空。水平起点、朝向和观察面来自每个用例的轨迹。直接源使用紧邻活塞侧面的红石块。QC 源固定为 `P + up + north`，活塞与源之间的相邻格留空。隔离的有效 NC 从 `P + down` 放置或移除玻璃产生。记录命令实际触达的范围，添加组合时保留此基线作对照。

变体运行前重新创建相同初始状态并确认活动区块。朝向变体绕竖直轴同时旋转方块与相对坐标，再添加专用的竖直活塞装置。区块变体将基座、QC 源或头部置于 `15 → 16` 或 `-1 → 0` 接缝两侧，高度另做 Section 接缝。已移动、推拉受阻和位于区块外的装置各自作为控制组。

<a name="bud-cases"></a>
## QC 与 BUD 用例

| ID | 操作和观察 |
|---|---|
| BUD-001 | 放置活塞，稳定后加直接源。在命令返回、每个 GT 末和移动完成后读取基座、头部及推送负载。 |
| BUD-002 | 同一 GT 先置 QC 源，再从另一面发有效 NC。记录立即状态、事件是否入队、动作在哪一 GT 生效。 |
| BUD-003 | 只置 QC 源，至少等待 5 GT。基座保持原状。用相同装置的阳性对照证明来源和空间位置确实可用。 |
| BUD-004 | 完成 BUD-003 后保持供电并通知，观察伸出、移动占位和完成顺序。 |
| BUD-005 | 从稳定伸出的 QC 活塞移除源，避免直接通知。逐 GT 检查是否保持伸出。 |
| BUD-006 | 完成 BUD-005 后发送有效 NC。观察收回中、合法运动结束及原位状态。 |
| BUD-007 | 同装置逐一改变远处、对角、邻格和上方测试方块。记录触发写入和随后动作，用无触发的位置作阴性对照。 |
| BUD-008 | 用原版专用入口分别发送 Shape Update 与 NC 并观察。RustCraft 增加派生缓存失效对照，确认无对应通知和动作。 |
| BUD-009 | 单 GT 内做开、关、再开及相反次序，按每次指令返回和 GT 结束采样。区分最终供电相同但因果不同的情况。 |
| BUD-010 | 发送有效 NC 安排伸出，在步进前撤源。观察撤源前后的队列与 GT 结束时的实际状态，核对是否在执行阶段重查。 |
| BUD-011 | 至少测四个水平方向、一个上向、一个下向。记录旋转及镜像变换、负坐标和相对次序。 |
| BUD-012 | 分开保存 QC 未通知、已通知待动作、运动中和运动完成后的世界。真实退出及重新打开，检查状态和事件的生命周期。[已有独立进程差分](#qc-restart-evidence)。 |
| BUD-013 | 横跨正负 Chunk 和垂直 Section。分别比较活动、尚未加载、恢复活动与维度暂停时的样本。 |
| BUD-014 | 使用同一 RustCraft 输入和检查点，运行禁用/启用连接缓存的配对测试，对比更新、顺序、动作和结果。 |
| BUD-015 | 在 RustCraft 参考重复运行中扰乱后台任务衔接顺序，检查提交轨迹相同。检测过时结果、跨区块写入和调度阻塞。 |

BUD-001 至 BUD-013 按组件组合测试普通和粘性活塞、发射器及投掷器。容器类同时记录触发位、待处理记录和实际输出。当前六向普通活塞用例使用两个正向、两个负向的 Chunk 接缝及上下跨 Section 装置，均在活动区块。BUD-014 和 BUD-015 等对应优化实现后启用，以可运行的结果验证等价。

## 时序与顺序用例

| ID | 试验操作与采样 |
|---|---|
| TIM-REPEATER-1–4 | 平滑石头上放朝西的中继器，源置于其西侧。同一 GT 记录输入后状态，再观测每 GT 的首次变更。分别从奇数和偶数 GT 触发，旋转并测试关断及锁定。 |
| TIM-REPEATER-SHORT、PREPOWER、PLACED-BY | 比较 1 GT 输入产生的计划边沿、有输入时的命令写入，以及有输入时的放置回调。 |
| TIM-REPEATER-LOCK、PRIORITY、BATCH、NOTIFY | 检查侧锁两种极性、相邻中继器朝向、同 GT 到期批次，以及通知范围内外的 QC 活塞。 |
| TIM-DIODE-PRIORITY | 让两个计划刻在同一 GT 到期，同时观测优先级、插入顺序、同/跨 Chunk、前级部件、锁定和邻近并行电路。 |
| TIM-COMPARATOR | 比较与减法、玩家切换、模拟强度、侧向粉末和弱信号、优先级、带电放置及比较器侧锁。 |
| TIM-COMPARATOR-THROUGH、OUTPUT、RESTART-COMPARATOR | 专用更新穿过导体，确认模拟强度对粉末和观察者的不同影响，再验证替换和实际存档重开。 |
| TIM-TORCH | 立式和四向壁挂的输出、1 GT 输入复查、支撑移除、强弱充能及放置附着面。 |
| TIM-TORCH-BURNOUT、RESTART-TORCH | 让第八次熄灭距第一次分别为 59、60 和 61 GT。比较有无恢复刻，并在独立原版世界保存重开。 |
| TIM-OBSERVER | 观测面、非观测面、脉冲期间重触发、六向放置、带电替换及方向性输出。 |
| TIM-OBSERVER-CHAIN、CIRCUIT | 两个观察者跨 Chunk 触发红石灯，以及观察者经粉末和中继器触发跨 Chunk 活塞。记录两种边沿、计划刻次序和负载状态。 |
| TIM-LEVER | 用原版放置入口查询六个支撑方向，并在地面、墙面及顶面试验中检查翻转、六向供电、支撑传导和移除。 |
| TIM-BUTTON | 石及橡木按钮的地面、墙面和顶面安装，记录重复点击、附着方块强充能、支撑移除及释放期限。 |
| TIM-PLATE | 单个原版盔甲架进入、保持和离开石、橡木和两种测重压力板。RustCraft 用玩家包围盒重放中心位置的接触。 |
| TIM-EJECTOR | 发射器和投掷器接收直供和准连接通知，记录触发态、延迟、跨 Chunk 吐出、容器转移和比较器读数。 |
| TIM-WIRE-EXTERNAL | 从上下方电源和强充能支撑块为尘线供电。检查衰减、撤电和隔导体的粉末反馈。 |
| SHAPE-WIRE | 比较物品放置与直接状态写入，重放点与十字切换、拐角、T 形和跨 Chunk 石材及玻璃台阶。 |
| TIM-WIRE-NOTIFY | 粉末经石块点亮多个间隔一格的灯，记录观察者的脉冲，以及撤电或移除带电粉末后各灯的入队次序。 |
| TIM-PISTON | 在输入前后、Block Event 阶段和移动完成处读基座、头部、负载。比较长脉冲、1 GT 脉冲和同刻反转，以及普通与粘性。 |
| TIM-PHASE | 分别在命令输入、计划刻入口、Block Event、移动完成时触发后级元件，比较同 GT 内可见差异和后续实际延迟。 |
| ORD-NOTIFY | 使用隔离命令及不同更新标志测 NC、Shape、Comparator、嵌套通知、重复通知、链式上限和各方向顺序。 |
| ORDER-CHUNK | 交错插入同刻计划任务，包含分块到期、同位置去重、不同方块类型、跨区块先后和保存重载后的持续序列。 |
| PWR-CONDUCTION | 对比普通输出与直接输出。用强充能、弱充能和非导电方块隔开源与尘线、灯、二极管。已有拉杆、红石块、两格尘线和平滑石头与玻璃的六面查询。灯、二极管及动作判定待测。 |

<a name="order-evidence"></a>
### 计划刻排序的首个差分

`ORDER-CHUNK` 在原版 `LevelTicks` 上记录五次入队尝试，含较晚到期的极高优先级任务、较早到期的普通任务、相邻 Chunk 的高优先级任务和同类型同位置的重复入队。在最后一个到期 GT 收集，执行顺序为 `other_high → older_normal → other_normal → urgent_later`。这个结果同时检验跨 Chunk 选择、Chunk 内到期顺序和重复抑制。RustCraft 独立的 Java 方块刻参考队列直接消费 `schedule_attempt` 记录并与原版顺序比较。

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference -- --ignored
```

把 `XXXXX` 的目录替换为脚本打印的实际目录。常规工作区测试包含固定输入的 RustCraft 顺序回归。原版差分另行读取版本固定的采样。邻居方向数组也读到了西、东、下、上、北、南。嵌套通知次序另行验证。[LevelTicks 映射](https://mappings.dev/1.20.1/net/minecraft/world/ticks/LevelTicks.html) · [NeighborUpdater 映射](https://mappings.dev/1.20.1/net/minecraft/world/level/redstone/NeighborUpdater.html)

<a name="repeater-evidence"></a>
### 中继器计划刻和通知

实测四档升降沿分别等待 2、4、6、8 GT，默认优先级为升沿 −1、降沿 −2。输出格放置同向或横向的中继器时，通知触发的计划刻优先级为 −3。输出对接时使用默认优先级。两个同 GT 到期的中继器先执行优先级更高的上游刻，下游保持原有到期事件，队列随后为空。

3 档中继器收到 1 GT 输入，按计划在输入开始后 6 GT 接通信号，并再保持 6 GT。侧向红石块不会锁定；朝向目标的中继器能锁定开、关两种状态，解锁后再以目标档位安排边沿。原版 `setblock` 写入已有输入的中继器没有安排启动；随后独立调用放置回调可观察到普通优先级的 1 GT 启动。RustCraft 用正常玩家放置重放后者。

输出切换后，东面隔一格的 QC 活塞与中继器在同一 GT 内启动，南面相邻的 QC 活塞保持缩回。测试逐项对比 141 条状态和 98 个待办快照，另有 RustCraft 保存重开测试核对短脉冲的两条边沿。比较器侧锁另见[比较器差分](#comparator-evidence)。原版中继器重开和完整通知链仍待验证。

<a name="comparator-evidence"></a>
### 比较器与模拟输出

背面输入为 12、侧面为 10 时，比较模式输出 12，右键切到减法模式后立即输出 2。两侧相等时，比较模式保持输出。侧面放置的地面拉杆只有弱信号，侧向读取为 0。红石块被读作 15，14 级粉末在减法模式下把 15 降到 1。面向中继器锁定侧的 2 级比较器输出可以锁住中继器。

输入变化的普通计划刻延后 2 GT，特定朝向的前置二极管使优先级升到 −1。带电放置回调安排普通优先级的 1 GT 启动刻。蛋糕相邻时直接提供模拟输出，隔平滑石头时需要有效通知。专用 Comparator Update 能穿过该导体，玻璃会阻断。导体收到 15 级输入时，背后的蛋糕强度不再覆盖它。[Level 比较器更新入口](https://mappings.dev/1.20.1/net/minecraft/world/level/Level.html)

比较器从 12 降到 6 时，跨 Chunk 的红石粉更新，观察比较器的观察者没有新增脉冲。移除并放回比较器会清除旧强度，尚未到期的同类型计划刻随后可以计算新输出。运行对照覆盖 171 条状态、171 个队列快照和 40 次供电查询。

原版磁盘重开专项在输出 12、下一次更新已入队时保存。重开后首两个样本保持 12，到期刻变成 6。RustCraft 额外验证停用区块仍能保存对应的强度和待办，重新激活后继续按截止时间更新。

<a name="observer-evidence"></a>
### 观察者及混合线路

观察面单独收到 NC 时队列为空。原版同一位置的 Shape Update 即使携带原有邻格状态，也会排入普通优先级的启动事件。第 2 GT 变为充能，第 4 GT 结束脉冲。输出侧变化、启动等待期内的新变化及脉冲期间的新变化均没有新增脉冲。输出面的普通和直接信号均为 15，其余面为 0。立在观察者上的拉杆未通过观察者传导信号。

用 `setblock` 写入未充能状态会安排一次初始脉冲。新写入的充能状态在无待办时归为未充能。使用物品放置的更新标志和回调不会产生初始脉冲。朝向取最近的视线方向，输出在背面。脉冲期间移除并在截止前放回同型观察者，原待办仍能在截止时执行。

跨 Chunk 串接观察者时，下游升沿在上游降沿前入队，两者在同一 GT 到期。另一条线路由观察者、红石粉、2 档中继器、朝东活塞和平滑石头构成，记录了输出脉冲、活塞移动占位和负载穿过接缝的状态。RustCraft 对照 163 条状态、170 个队列快照、12 次供电查询和 6 个放置方向，对比队列截止、优先级和可观察插入顺序。RustCraft 检查点另行验证了上、下降沿的恢复。原版卸载、退出和重载属于独立试验。

<a name="torch-evidence"></a>
### 红石火把与烧毁

立式火把读取下方支撑，壁挂读取背后的支撑。相邻红石块的弱输出未通过平滑石头传导。中继器向支撑发出直接信号时，壁挂火把收到通知并在 2 GT 后熄灭。1 GT 输入消失后，已排入的回调仍到期，继续保持点亮。玻璃支撑在所测的立式及四向壁挂装置中均可用。俯看时点击墙，原版物品放置仍选中墙面，即使下方也可支撑。

第八次熄灭与第一次相隔 59 或 60 GT 时安排 160 GT 后恢复，相隔 61 GT 则按普通熄灭处理。复查时仍有输入会保持熄灭；撤去输入后再等 2 GT。时序比较覆盖 186 条状态和 187 个队列快照，另有 60 次方向性供电查询。

原版 GameTest 在独立目录保存世界、关闭 Level，再重开实际地形数据。用两个火把分别验证已烧毁装置保留恢复刻，以及曾熄灭七次的装置在重开后重新计数。差分按重开后的相对 GT 比较剩余期限。RustCraft 另有经真实磁盘检查点重开壁挂火把的测试。完整通知链与区块卸载专项仍待验证。

<a name="power-evidence"></a>
### 方向性供电差分

同一命令也运行 48 次原版供电对照。查询输入是原版观察到的方块状态，比较普通输出、直接输出和含传导的输出。落地拉杆的直接输出指向支撑面，支撑石头能为侧面的尘线供电。红石块的普通输出为 15，直接输出为 0。平滑石头接收第二格红石粉的 14 级输出后，普通查询可看到传导电力，隔着该石头的第三格红石粉仍为 0。RustCraft 查询通过独立的尘线外部供电上下文实现这种区别。差分覆盖查询本身。尘线形状生成、实际传播、激活和通知使用后续专用差分。[SignalGetter 映射](https://mappings.dev/1.20.1/net/minecraft/world/level/SignalGetter.html)

<a name="wire-external-evidence"></a>
### 红石粉外部电源

红石块位于粉末正上方或正下方时，该格粉末得到 15 级信号。相邻 Chunk 的下一格得到 14 级，移除上方电源后两格同 GT 归零。拉杆强充能的石块可使石块旁的粉末得到 15 级信号。普通供电查询可测到尘线对石块的 15 级传导，另一侧的尘线保持 0。差分覆盖 21 项状态、8 项队列和 1 次供电查询。

<a name="wire-shape-evidence"></a>
### 红石粉连接与方向性输出

孤立粉末通过物品放置为十字，直接写入默认状态为点。右键可切换孤立粉末的点和十字，电平保持原值。点只向下输出，十字也向水平四面输出。相邻粉末可形成直线、拐角和 T 形，右键拐角保持其连接。移除最后一段连接后，试验中的粉末恢复十字。

石材和玻璃台阶均可在低处一侧出现 `up` 状态。封住低处粉末的上方时，平滑石头阻断台阶连接，玻璃允许连接。低处供电时，两种台阶的高处均得到衰减一级的信号。高处供电时，平滑石台阶的低处得到衰减信号，玻璃台阶的低处保持 0。差分直接比较四面的 `none`、`side`、`up` 属性和电平，覆盖 41 项状态及 180 次方向性供电查询。RustCraft 另测了带电点和十字的磁盘重开及渲染网格。[1.20.1 红石粉映射](https://maven.fabricmc.net/docs/yarn-1.20.1%2Bbuild.8/net/minecraft/block/RedstoneWireBlock.html)

<a name="wire-notice-evidence"></a>
### 红石粉电平改变后的通知

粉末电平变化后，隔一格平滑石头的灯可立即点亮，观察者也能由形状通知排入脉冲。撤电后的五灯装置在 `(8,300,3)` 处按下、南、东、北、西入队，在 `(24,308,4)` 处按北、西、下、南、东入队。RustCraft 以 Java 17 散列表中七个通知中心的桶序复现这些次序。

保持电源并移除粉末，底部灯和东侧灯先后排入 4 GT 熄灭刻，观察者随后排入 2 GT 脉冲。两组五灯样本和三组含观察者的装置共比较 167 项状态与 171 项待办快照。完整嵌套链与上限仍需专项实验。

<a name="bud007-evidence"></a>
### BUD-007 更新范围矩阵

`BUD007R-*` 使用同一个朝东、QC 供电且未收到有效通知的普通活塞，依次在九个相对位置放置玻璃，记录是否产生活塞 Block Event：

| 位置 | 结果 |
|---|---|
| 下方 | 触发 |
| 北、南、西三个相邻面 | 触发 |
| 前方/头部格 | 触发 |
| 上方 | 触发 |
| 两格远东 | 不触发 |
| 对角 | 不触发 |
| 两格远北 | 不触发 |

Rust 差分回放覆盖 9 项状态和 18 项队列快照，无进度样本：

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  matches_bud007 -- --ignored
```

方向的扩展矩阵 `BUD007D-*` 覆盖东、南、西、北四个朝向上的下方、两格前方和对角位置：四个朝向的下方变化都触发，两格前方和对角均不触发。`BUD007S-seam` 将活塞放在 Chunk 接缝一侧，对两格外变化保持不触发。Rust 差分回放分别通过 9/18、12/24 和 1/2 项状态/队列样本。

<a name="bud007-chain-evidence"></a>
### BUD-007 红石粉级联通知顺序

`BUD007L-*` 用五段红石粉构成一条东西向链，每段粉旁放一个由独立红石块 QC 供电的活塞探针。放置西端电源块后，粉链逐段改变功率，活塞探针按收到通知的顺序进入 Block Event 队列。

原版与 Rust 现在都得到：

```text
172, 174, 173, 175, 171   （活塞 base 的 x 坐标）
```

最初 Rust 回放为 `172,171,173,174,175`，差分工具定位到第二个事件的首个偏差。原因是 Rust 只在处理完一个通知中心后排空新编辑，而 Java 在 `updateNeighborsAt` 的每个邻格回调中同步递归 `setBlock`。现在 `EditNotification::WirePower` 在每个邻居通知后立即分派新产生的编辑，使粉链顺序与原版一致；同一改动下发射器和投掷器的同 GT 测试仍通过。

Rust 差分回放覆盖 26 项状态和 3 项顺序队列快照：

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  matches_bud007_dust_cascade_and_piston_probe_order -- --ignored
```

扩展拓扑继续覆盖 9 段直线、双端电源和 L 形转角：

| 场景 | 拓扑 | 原版与 Rust 的活塞事件顺序 | 样本 |
|---|---|---|---|
| `BUD007T-long` | 9 段直线，跨 x=175/176 Chunk 接缝 | `171,173,175,177,179,178,176,174,172` | 36 状态 / 3 顺序队列 |
| `BUD007T-multi` | 9 段直线，西端与东端电源同 GT 放置 | `172,173,174,176,177,178,179,175,171` | 36 / 3 |
| `BUD007T-corner` | L 形，3 段横向 + 转角 + 3 段纵向 | `(181,131),(183,131),(185,131),(185,133),(185,132),(182,131)` | 25 / 3 |
| `BUD007T-junction` | 主链 + 南北双分支 T 形 | `(182,151),(183,151),(184,151),(186,151),(186,149),(186,148),(186,147),(186,152),(186,153),(181,151)` | 41 / 3 |
| `BUD007T-section` | 下行 y=15 经台阶抬升到 y=16，跨 Section | `(191,15,161),(193,15,161),(192,15,161),(194,16,161),(195,16,161),(196,16,161)` | 18 / 3 |

五个扩展场景的 Rust 回放均通过。L 形转角使用南向和东向两组活塞探针，T 形使用主链南向探针和两条分支的东向探针，按完整坐标比较顺序。跨 Section 场景只比较供电后的状态和顺序；放置形状由既有 `SHAPE-WIRE` 场景覆盖。当前仍未覆盖 13 段以上长链、更多分叉和卸载/重载链。

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  matches_bud007_extended_dust_cascades -- --ignored
```

组件矩阵 `BUD007C-*` 将同样的下方、两格前方和对角变化应用到粘性活塞、发射器和投掷器：

| 组件 | 下方变化 | 两格前方 | 对角 |
|---|---|---|---|
| 粘性活塞 | 产生活塞 Block Event | 不产生 | 不产生 |
| 发射器 | 安排计划刻，`triggered=true` | 不安排，`triggered=false` | 不安排，`triggered=false` |
| 投掷器 | 安排计划刻，`triggered=true` | 不安排，`triggered=false` | 不安排，`triggered=false` |

Rust 差分回放分别通过 3/6 项状态/队列样本。发射器和投掷器按 Scheduled Tick 队列比较，粘性活塞按 Block Event 队列比较，两者未混为同一队列。

当前矩阵覆盖普通/粘性活塞、发射器、投掷器、单一 QC 源和活动区块；发射器/投掷器的物品转移效果不在此矩阵断言范围内，更多 Chunk/Section 接缝、更长/分支/多源链仍待扩展。

<a name="component-bud-evidence"></a>
### 组件级 Shape/NC 与同 GT 供电变化

新增 `BUDX-STICKY-008/009`、`BUDX-DISPENSER-008/009` 和 `BUDX-DROPPER-008/009`。装置均为朝东、活动区块和单一 QC 源位置，分别记录 Shape Update、Neighbor Changed、同 GT 开/关/再开、计划刻队列、库存和射出物。

- BUD-008：只有 QC 供电时保持未激活；单独的 Shape Update 不安排动作；有效 NC 后才安排动作。粘性活塞随后伸出，发射器和投掷器在 4 GT 后各射出一件。
- BUD-009：同一 GT 内依次供电、撤源、再供电并发送 NC，队列保留一个动作；粘性活塞下一 GT 伸出，发射器和投掷器在 4 GT 后各射出一件。
- RustCraft 差分通过：粘性活塞 11 项状态和 7 项队列；发射器、投掷器各 14 项状态和 14 项队列，另含库存与射出物观察。

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  bud_cases -- --ignored
```

本节只覆盖四类组件的朝东选定装置和活动区块，不代表全部 BUD-008/009 变体。同一参考运行的 `CHUNK-PISTON`/`CHUNK-TIM` 阶段仍未形成可信的在线重载证据，继续作为独立 TBD，不与本节结论混用。

`chunk-residency` 夹具的阻塞原因已定位：它用 level-33 resident ticket 保留区块数据，再通过 `setChunkForced` 恢复；GameTest 服务器没有玩家连接，因此重新激活后虽然 ticket level 已恢复，block ticking/block entity ticking 仍不推进。尝试补 forced/player ticket、重建 `LevelTicks` 容器和标记 entity-ticking 均不能形成可信的原版 tick 序列。追加尝试用 `EmbeddedChannel` 构造 `ServerPlayer` 连接对象也没有建立 GameTest 的 PlayerList/ticket，不能恢复 ticking。该夹具不能作为 BUD-013 证据。

<a name="dedicated-bud013"></a>
### DedicatedServer 在线区块探测

为恢复真实玩家 ticket，新增 `tools/run_dedicated_bud013.py`。脚本从已安装的 1.20.1 客户端 JAR 启动 offline-mode DedicatedServer，使用最小协议 bot 加入，并在目标区块执行离开/返回场景。命令从仓库根目录运行：

```sh
python3 -B tools/run_dedicated_bud013.py --java /opt/openjdk-bin-17/bin/java
```

同一脚本通过 `-javaagent` 附加 `tools/java-redstone-reference/BudAgent.java`。agent 在每个 GT 通过 `MinecraftServer.execute` 把快照任务提交到 Server 线程，事务内读取 `ServerLevel` 的 `LevelTicks`、Block Event 和目标 Chunk 的 `ticking/ticket/resident` 状态；后台线程只写已完成的 JSONL，不直接读取 Minecraft 队列。agent 不修改方块、队列或实体。

最近一次本地运行输出：

- 红石灯供电后 `lit=true`。
- 玩家离开后，目标区块查询返回 `That position is not loaded`，说明在线区块已卸载。
- 玩家返回后灯为 `lit=false`，计划刻熄灭在重新加载后可见。
- 活塞在玩家离开前安排伸出；返回后基座 `extended=true`、头部存在、负载仍在。
- agent 轨迹在 GT 35–62 持续记录 `[264,110,6]` 的红石灯计划刻，`due` 为 38–39（取决于命令相位）；该区间区块查询为未加载。
- agent 轨迹在 GT 104–147 持续记录 `[264,110,10]` 的活塞 Block Event，`id=0`、`data=5`；返回后事件消失且活塞完成伸出。
- 跨 Section 的朝上活塞在 GT 177–220 保留 `[264,15,6]` 的 Block Event，返回后基座、头部和负载状态完成。
- 五段红石粉链与 QC 活塞探针在 GT 355–440 期间保留 5 条 Block Event；agent 观察到目标区块 `resident=false`，事件顺序为 `[264,110,2]` 到 `[268,110,2]`，返回后 5 个活塞全部伸出、头部存在且队列清除。

轨迹写入 `target/dedicated-bud013.*/trace.jsonl` 和同目录的 `agent.jsonl`。该探测使用随机本地端口、offline-mode 和原版 1.20.1 服务端，不修改服务端 JAR。agent 只按 GT 采样，不能还原同一 GT 内的完整调用次序；重复运行、其它元件和 Rust 逐事件自动差分仍需补齐。尝试用 160 GT 火把恢复证明长延迟事件跨卸载保留时，服务端夹具没有形成可靠 burnout，因此该场景暂不作为证据。

自动队列校验：

```sh
python3 -B tools/check_bud013_trace.py \
  "$(ls -dt target/dedicated-bud013.* | head -1)"
```

检查器验证灯/普通活塞/跨 Section 活塞的队列条目在 `loaded=false` 窗口内保留，并在返回后清除。最近一次输出：

```text
PASS DEDICATED-BUD013-LAMP block_ticks retained across unload and cleared after return
PASS DEDICATED-BUD013-PISTON block_events retained across unload and cleared after return
PASS DEDICATED-BUD013-SECTION block_events retained across unload and cleared after return
PASS DEDICATED-BUD013-CHAIN block_events retained, ordered and cleared after return
```

对应的 Rust 回归：

```sh
cargo test --offline --locked -p rustcraft-authority --test java_flat \
  preserves_ -- --nocapture
```

测试在 `SetChunkActive(false/true)` 期间保留灯计划刻、普通活塞 Block Event 和跨 Section 活塞 Block Event，并验证返回后的最终状态。

Rust 侧逐 GT 队列导出与 Java/Rust 自动比较：

```sh
cargo run --offline --locked -p rustcraft-authority --example bud013_trace \
  > target/rust-bud013.jsonl
python3 -B tools/check_bud013_trace.py \
  "$(ls -dt target/dedicated-bud013.* | head -1)" target/rust-bud013.jsonl
```

最近一次输出：

```text
PASS java DEDICATED-BUD013-LAMP {'retained': True, 'cleared': True, 'final': True}
PASS java DEDICATED-BUD013-PISTON {'retained': True, 'cleared': True, 'final': True}
PASS java DEDICATED-BUD013-SECTION {'retained': True, 'cleared': True, 'final': True}
PASS java DEDICATED-BUD013-CHAIN {'retained': True, 'cleared': True, 'final': True, 'order': [264,265,266,267,268]}
PASS rust DEDICATED-BUD013-LAMP {'retained': True, 'cleared': True, 'final': True}
PASS rust DEDICATED-BUD013-PISTON {'retained': True, 'cleared': True, 'final': True}
PASS rust DEDICATED-BUD013-SECTION {'retained': True, 'cleared': True, 'final': True}
PASS rust DEDICATED-BUD013-CHAIN {'retained': True, 'cleared': True, 'final': True, 'order': [264,265,266,267,268]}
PASS Java/Rust BUD-013 queue comparison
```

该比较是队列窗口级：验证同一位置的事件在卸载期间保留、返回后清除、最终状态一致，并新增五段粉链的 Block Event 相对顺序比较；尚未逐 GT 比较完整事件序列，也未覆盖粘性活塞、发射器、投掷器、多区块部分卸载和全部方向。

### 双 Chunk 单侧停用对照

`Reference.chunkResidency()` 末尾增加 `CHUNK-PARTIAL`：在 Chunk `(15,0)` 的 x=254 和 Chunk `(20,0)` 的 x=323 各放置一个 QC 供电活塞，并通过相邻玻璃通知排入 Block Event。两个 Chunk 初始均处于 ticking 且 resident。随后只停用 `(20,0)`，保持 `(15,0)` ticking：到 GT 19，x=254 活塞已经伸出且队列清空，x=323 仍缩回并保留一个事件。重新启用 `(20,0)` 后，到 GT 23 两个活塞都已伸出，两个队列均清空。

隔开 Chunk 15 与 20 是为了避免邻接强制票把停用 Chunk 再次带入 ticking。GameTest 记录两侧始终 `resident=true`，所以该结果只证明常驻区块间独立暂停/恢复及 Rust 对照，不证明 chunk unload/load 语义。通过同一 reference trace 重放的忽略测试：

```sh
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.<run>/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  matches_piston_events_while_only_one_of_two_chunks_is_inactive -- --ignored --exact
```

<a name="notification-agent-smoke"></a>
### 通知级 Java agent 冒烟

`tools/java-redstone-reference/NotificationAgent.java` 通过 JDK internal ASM 只读插桩 Level 和 BlockStateBase 的通知入口，不修改参数、返回值、调度或世界状态。`tools/run_notification_smoke.py` 在简单活塞场景做有/无插桩 A/B：

```sh
python3 -B tools/run_notification_smoke.py \
  --prism-root "$HOME/.local/share/PrismLauncher" \
  --java /opt/openjdk-bin-17/bin/java
```

最近一次输出：

```text
PASS A/B behavior: final=(True, True, True) retained=True cleared=True
PASS notification smoke: events=134 kinds=['neighbor_changed', 'neighbor_shape_changed', 'update_shape'] max_depth=1 gt=25 dropped=0
CHUNK away={'ticking': False, 'ticket': False, 'resident': False}
CHUNK return={'ticking': False, 'ticket': True, 'resident': True}
```

已验证的插桩入口包括 Level 的 `a(gu, cpn)`、`a(gu, cpn, ha)`、`a(gu, cpn, gu)`、`a(ha, dcb, gu, gu, int, int)`，以及 BlockStateBase 的 `a(cmm, gu, cpn, gu, boolean)` 和 `a(ha, dcb, cmn, gu, gu)`。冒烟观测到 `neighbor_shape_changed` 分发到嵌套 `update_shape`，以及 `neighbor_changed` 行为入口；`update_neighbors_at` 在该路径未命中，只作为辅助诊断。

该冒烟只证明通知级测量工具可用：enter/exit 配对、深度/父 ID、有界缓冲和 A/B 行为一致。它本身不验证 BUD-008/009 的多路径；后续对应矩阵见下一节。`ticking` 字段也不能直接等同于 Block Event 可处理状态。

GameTest 离线参考运行也可输出完整通知轨迹，避免 DedicatedServer 本地端口不可用时失去父子调用证据：

```sh
python3 -B tools/run_java_redstone_reference.py \
  --prism-root "$HOME/.local/share/PrismLauncher" \
  --java /opt/openjdk-bin-17/bin/java --notifications

python3 -B tools/check_notification_trace.py \
  target/java-redstone-reference.<run>/notifications.jsonl
```

校验器要求 enter/exit 成对、父 ID 和深度匹配、缓冲无丢失，并检查原版轨迹确实出现 `neighbor_shape_changed → update_shape`、`neighbor_shape_changed → neighbor_changed` 与 `neighbor_shape_changed → neighbor_shape_changed` 三类嵌套路径。它报告观测到的最大调用深度，不推断未观测到的更深递归；通知轨迹是原版证据，尚不等同于 Rust 全部更新链的差分覆盖。

<a name="bud008-dual-source-baseline"></a>
### BUD-008/009 双来源 NC/Shape Java 基准

`tools/run_bud008_matrix.py` 用 Datapack 函数在同一 GT 内顺序执行 `setblock`，分别运行 A-only、B-only、A→B、B→A 和同源重复来源场景。没有使用命令方块、schedule 或强制 Tick。每个场景先放置单个 QC 供电活塞，再通过函数设置来源：

- A：活塞下方 `[258,109,12]` 放置玻璃。
- B：活塞上方 `[258,111,12]` 放置玻璃。

控制函数在 `[270,110,2]` 和 `[270,110,8]` 放置两个无关玻璃块；两次运行的 Server 线程 GT 均为同一值（样本 17、18），证明函数内的两条 setblock 可以落在同一 GT。

双来源结果（两次运行均通过）：

| 场景 | 首个到达活塞的来源 | 初始来源通知 GT | 活塞 Block Event 数 | 最终状态 |
|---|---|---|---|---|
| A-only | A | 同 GT | 1 | `extended=true`、头部存在 |
| B-only | B | 同 GT | 1 | `extended=true`、头部存在 |
| A→B | A | 同 GT | 1 | `extended=true`、头部存在 |
| B→A | B | 同 GT | 1 | `extended=true`、头部存在 |

每个来源的初始链均为 `neighbor_shape_changed`（depth 0）→ 嵌套 `update_shape`（depth 1）→ `neighbor_changed`（depth 0）。A、B 两条链串行执行，彼此没有父子关系；同一 GT 内两条路径都抵达活塞，但 Block Event 队列只保留 1 个事件。GT 40/43 等来自活塞头和运动的后续通知已按来源过滤，不计入初始双来源轨迹。

GT 改为在 Server 线程读取后，AB/BA 的初始来源通知在两次运行中均同 GT；此前后台时钟辅助读数出现的 GT 35/36 拆分没有复现，判断为观测时钟问题而非函数跨 Tick。Chunk 状态仍显示 away `resident=false`、return `resident=true/ticket=true`，事件执行通过返回后的活塞状态独立验证。

同源重复另由离线 GameTest 参考入口 `Reference.repeatedSourceNotices()` 复现：底部来源在 GT 41 内依次玻璃→空气→玻璃。使用 `--notifications` 运行会附加只读 `NotificationAgent`。本次运行 `target/java-redstone-reference.nmyxbz_r/notifications.jsonl` 在活塞目标处记录了 3 次 `neighbor_changed`，并在来源每次变化时记录 1 次朝下的 Shape 分发；`reference.jsonl` 的 `after_first_notice`、`after_second_notice`、`after_third_notice` 队列快照均为 1 项，GT 41 活塞仍缩回，GT 42 活塞伸出。这里的 Java 观测证明通知次数、顺序和队列基数；它不声称直接观测到原版内部尝试插入后被抑制的 Block Event 次数。

六面方向矩阵由 `Reference.shapeAndNeighborFaces()` 采样：玻璃来源位于活塞的 west/east/down/up/north/south 六个相邻位置。每个来源的 Shape 单独通知后，活塞队列仍为空且下一 GT 仍缩回；随后同一方向的 NC 各排入一个事件，并于下一 GT 伸出。RustCraft 的同名方向回归对六面重复相同检查，结果通过。

排除面矩阵由 `Reference.neighborExclusionFaces()` 对六个方向分别调用 `Level.updateNeighborsAtExceptFromFacing`。在 `NotificationAgent` 轨迹中，来源方块放置本身先通知全部六邻居；紧随显式排除调用的各组仅有五个 NC 目标，目标顺序等于原版 `west,east,down,up,north,south` 去掉指定面。例如排除 west 时剩余 east、down、up、north、south；排除 down 时剩余 west、east、up、north、south。Rust 的 `neighbor_faces_except` 将这一稳定顺序用于定向输出与红石粉切换通知，`NotifyJavaNeighborsExcept` 事务入口回放相同参考输入；36 组（排除面×观察目标面）回归验证目标只在未排除时收到 NC。

RustCraft 回归 `crates/authority/tests/bud008_dual_source.rs` 在匹配的单来源重复输入和双来源场景下断言：

- `BudWatch` 的 `Check` 来源顺序与 A/B 一致。
- 同一场景的各来源 `Check` 在同一逻辑 GT。
- `Shape` 记录方向为 A 的下方、B 的上方。
- 待执行活塞 Block Event 恰好 1 个。
- 同源重复时 3 次检查和 3 次 Shape 均保留，事件轨迹为 1 次 `Queued`、2 次 `Duplicate`。
- 执行后活塞为 `piston_extended`，事件队列清空。

Rust 回归通过。

自动差分链路：

```sh
python3 -B tools/run_bud008_matrix.py --java /opt/openjdk-bin-17/bin/java
cargo run --offline --locked -p rustcraft-authority --example bud008_trace \
  > target/rust-bud008.jsonl
python3 -B tools/compare_bud008_trace.py \
  target/bud008-java.json target/rust-bud008.jsonl
python3 -B tools/compare_bud008_trace.py --self-test
python3 -B tools/run_java_redstone_reference.py \
  --prism-root /home/Open-Source-Emperor/.local/share/PrismLauncher \
  --java /opt/openjdk-bin-17/bin/java --notifications
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  matches_bud008_ -- --ignored
RUSTCRAFT_JAVA_REDSTONE_REFERENCE="$PWD/target/java-redstone-reference.XXXXX/reference.jsonl" \
  cargo test --offline --locked -p rustcraft-authority --test java_redstone_reference \
  replays_bud008_neighbor_exclusion_inputs -- --ignored
```

双来源在线矩阵的最近一次结果：

```text
{"scenario": "a", "status": "MATCH"}
{"scenario": "b", "status": "MATCH"}
{"scenario": "ab", "status": "MATCH"}
{"scenario": "ba", "status": "MATCH"}
PASS comparator self-test: 11 cases
```

比较口径：

- Java 的 `neighbor_shape_changed` 分发和嵌套 `update_shape` 行为合并为一个语义 Shape 事件；父子关系用于确认是同一通知。
- NC 事件保留重复次数，不做无依据去重或重排。
- 比较使用相对 GT；Shape/NC 的来源、方向、顺序和 GT 偏移分别检查。
- Block Event 数量和最终活塞/头部状态独立断言。
- 缺少字段或 `UNKNOWN` 来源返回 `INCONCLUSIVE`，不会报告 MATCH。
- 自检覆盖完全一致、缺失、额外、顺序交换、重复次数、方向、GT 偏移、UNKNOWN、队列和最终状态十类注入差异。

同源重复的 Java 离线 GameTest 与 Rust 回归分别证实 3 次同 GT 来源通知、每次 1 项待处理队列，以及 Rust 侧 1 次入队加 2 次重复抑制；六向 Shape/NC 阴阳性对照和六面排除派发次序也分别有 Java 轨迹与 Rust 回归。比较器自检额外覆盖完整三组重复 Shape/NC 的 MATCH，并保留“Java 有两次通知、Rust 少一次”时的 MISMATCH 检查。双来源在线矩阵已完成 `first_divergence` MATCH；同源重复的在线矩阵差分和多层嵌套通知链仍待完成。

离线参考及通知轨迹可重采：

```sh
python3 -B tools/run_java_redstone_reference.py \
  --prism-root /home/Open-Source-Emperor/.local/share/PrismLauncher \
  --java /opt/openjdk-bin-17/bin/java --notifications
python3 -B tools/compare_bud008_trace.py --self-test
```

<a name="piston-evidence"></a>
### 活塞、QC 和红石灯的权威差分

同一命令从原版轨迹读取方块写入和有效通知，提交至 `JavaFlatV3` 的权限事务接口。每次原版观察前推进到对应的 GT。普通活塞用例比较 90 项块状态和 30 项待执行事件快照，涵盖无通知时保持、实际通知、Shape Update 的阴性对照、撤源、相同 GT 多次输入、运动占位以及六向边界。独立供电探针运行六个朝向和 14 种源位置。红石灯用例比较 61 项状态及 42 项队列观察，覆盖立即点亮、4 GT 熄灭与截止前重新供电。灯状态变化能触发观察者旁的形状检查，相邻 QC 活塞保持缩回。同刻熄灭的灯不会因另一盏灯熄灭而多出一次计划刻。

<a name="qc-restart-evidence"></a>
### QC 装置的独立进程重开

`qc-save` 在原版实际存档完成后退出，`qc-load` 在新 JVM 中加载同一目录。普通与粘性活塞各有七种场景，覆盖未通知升沿、排队伸出、两档伸出进度、收回运动、失电但未通知的伸出状态和排队收回。所有活塞朝东，底座位于 `x=15`，头部跨入 `x=16`；高度分别为 `y=15` 和 `31`，QC 源跨 Section，另覆盖正负 z 坐标。

原版重开后，未执行的伸出与收回事件均消失。未收到有效通知的状态保持，后续显式 NC 可重新排入动作。运动方块保留，但恢复的是上一 GT 的进度。保存前进度为 `(0, 0.5)` 时重开为 `(0, 0)`，首次 GT 结束后方块实体就绪，第四 GT 完成运动；保存前为 `(0.5, 1)` 时恢复为 `(0.5, 0.5)`，第三 GT 完成。记录同时包含区块能否执行方块刻和实体是否就绪，区分运动进度恢复与启动等待。[1.20.1 活塞运动状态接口](https://maven.fabricmc.net/docs/yarn-1.20.1%2Bbuild.5/net/minecraft/block/entity/PistonBlockEntity.html)

同一磁盘世界还包含发射器和投掷器各两台，朝东并紧邻 Chunk 接缝。每台槽位 3 放两块平滑石头。一组保持 QC 供电且未收到有效 NC，另一组在通知排入 4 GT 动作后撤电并再通知。重开保留库存和计划刻，已撤电组仍到期吐出一件；未通知组等待新的 NC 后才开始自己的 4 GT 延迟。原版重开会重新分配计划刻序号，差分在每个进程生命周期内比较两个容器的相对次序、截止 GT 和优先级。

V3 差分通过 356 项活塞状态、152 项事件队列和 51 项运动进度；容器专项通过状态、计划刻、库存及实体各 34 项观察。RustCraft 磁盘回归还验证保存不清空运行中的事件、重开后立即再保存不重复回退进度，以及负载和头部恢复。样本产物为 `target/java-redstone-reference.yg1ryr3y/reference.jsonl`，按上方命令可重新采集。在线区块卸载/重载、其他朝向及特殊物品仍待独立对照。

<a name="lever-evidence"></a>
### 拉杆安装与支撑充能

六个支撑方向的放置使用原版物品上下文和 RustCraft 的玩家放置事务。开关开启时，地面、墙面和顶面拉杆向六面输出普通信号 15，只向支撑方块输出直接信号 15。相邻灯和只接触支撑方块的灯均立即点亮。撤去信号会安排 4 GT 后熄灭，短暂断开再合上时沿用该到期刻，执行时灯保持亮起。用 `setblock` 移除顶面拉杆的支撑时，形状通知使拉杆先脱落，直连灯在支撑传导的灯之前入队，两者在同一 GT 到期。

直接用 `setblock` 写入开启状态会点亮直连灯。支撑方块虽有 15 级输出，经支撑供电的灯要等到后续有效 NC 才点亮。右键翻转会同时通知支撑方块周围。差分覆盖 113 项状态、98 项队列和 37 次方向性供电查询，包含灯位于相邻 Chunk 的线路。特殊支撑形状、命中盒及完整嵌套通知顺序仍待专项验证。

<a name="controls-evidence"></a>
### 按钮和压力板

石按钮与橡木按钮分别保持 20 GT、30 GT。重复点击石按钮时，已有释放刻按原期限执行。正常输出六面为 15，直接输出指向安装面。跨 Chunk 的两盏灯分别观察普通输出和经支撑传导的输出，在按钮松开后各等待 4 GT 熄灭。对照覆盖 37 条状态、35 个队列快照和 18 次方向性供电查询。

在原版的盔甲架接触试验中，石和橡木压力板输出 15，两种测重压力板均输出 1。短时间离开后，状态保持到下一次计划刻。普通板每 20 GT 复查，测重板每 10 GT 复查。RustCraft 用玩家的中心接触重放，比较 36 条状态、36 个队列快照和 24 次方向性查询。磁盘回归分别重开按下的按钮及处于检测周期的测重板。RustCraft 另用实际吐出的掉落物验证测重板触发、石板保持关闭。箭矢、多实体计数及压力板的原版掉落物和磁盘重开对照仍待补齐。

<a name="ejector-evidence"></a>
### 发射器、投掷器与掉落物

两种朝东容器在有效 NC 后立刻置触发态并安排 4 GT 后执行。只有准连接电源或收到形状通知时，容器保持原状态。先撤电并通知会清除触发态，已入队的动作会按期执行。触发态变化能让观察者排入脉冲，已有的观察者脉冲会按原截止时间执行。

投掷器优先向目标容器第一个可接收槽位转移一件，满目标会保留源物品。朝空位吐出时执行刻出现掉落物，跨过相邻 Chunk 接缝。容器库存从 42 件降到 41 件时，比较器在自己的计划刻从 2 降到 1。差分覆盖 67 项状态、65 项队列、4 次供电查询、15 项库存及 10 项实体观察。RustCraft 磁盘回归另行恢复待办、库存和飞行中的物品。独立进程重开另有[QC 和计划刻差分](#qc-restart-evidence)。其他朝向、特殊物品及随机分布仍待补齐。

<a name="bud-inspection"></a>
## 只读 QC 与 BUD 诊断

将正在检查的活塞底座、发射器或投掷器的坐标传给独立服务端。以下命令读取已发布的当前代，输出 NDJSON。

```sh
cargo run --offline --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- \
  --inspect-bud ./world 9 11 9
cargo run --offline --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- \
  --trace-bud ./world 9 11 9 5
```

`--inspect-bud` 仅读取快照。快照可报告供电、待办、运动和阻挡，已有失配因缺少运行历史显示为 `history_insufficient`。`--trace-bud` 在内存中从同一代推进指定 GT，重放存档内待办，并按序输出观察、形状通知、激活检查、动作和到期回调。两个操作均保留原存档和 `CURRENT`。

首行包含世界路径、检查点代、种子、维度和规则版本。每条事件含 GT、阶段、顺序号和可证实的父事件号。最后一行给出当前诊断。观察器保留最近 256 条单点记录，导出遇到缺口时写出缺失的序号范围。疑似缺少通知的报告带有一致基线及第一次发现失配的时间，基线离开窗口后降为历史不足。

回归用两个不同到期 GT 的任务依次写入准连接源和邻格，验证 `missing_update → pending → moving → consistent`，并比较两次进程回放的输出和检查点文件。另测了 Shape Update 无效、基岩和未加载区块阻挡、发射器和投掷器撤电后仍有到期待办、对象替换及证据淘汰。多点追踪、完整跨组件因果、全局状态摘要及客户端集成仍待完成。

<a name="trace-format"></a>
## 追踪与比较

RustCraft 权威事件使用 NDJSON。运行头固定规则 ID/实现版本、工具版本、世界/维度、初始检查点标识、种子、组件注册表、输入阶段及事件可见范围。下例是格式样例。

```json
{"schema":1,"record":"event","ruleset":"java/1.20.1","world":"reference","dimension":"minecraft:overworld","gt":41,"phase":"block_event","sequence":9,"event":"block_event_execute","parent":7,"source":[4,5,6],"target":[4,6,6],"direction":"up","before":"minecraft:piston[facing=east,extended=false]","after":"minecraft:piston[facing=east,extended=true]","power":{"normal":0,"direct":0},"activation":{"kind":"qc","source":[4,7,5]},"scheduled":null,"action":{"type":"extend","id":3,"outcome":"applied"},"observable_state_digest":"sha256:..."}
```

GT 内 `sequence` 由权威执行器产生，同时记录不同通知、候选、入队、执行、变更和派生失效。因果引用限定在同一世界/维度，跨 GT 的父键增加 GT。到期记录包含到期 GT、优先级、插入次序及队列身份。字段不可观测时省略或显式标为未知，零供电与未知供电分开。历史回放使用与服务端一致的提交结果，不因读取重新触发事件。

Reference Test 建立原版证据。Regression Test 在 RustCraft 锁定具体预设与失败重放。Differential Test 将双方的同一装置、输入投影、观察阶段和坐标归一到可共同观察的字段，错误输出首个不同的 GT、位置、输入、预期与实际状态，再附上能采集的因果。依次对照最终状态、逐 GT 序列、已观测的事件与因果、运动中间态以及长期机器。若原版针对某装置存在多种有效次序，先确定是环境变量还是可接受分布，再锁定 RustCraft 的确定性重放范围。
