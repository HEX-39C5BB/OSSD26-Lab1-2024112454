# 架构总览

[文档索引](README.md)

**文档版本：** 1.0 · 需求访谈收敛版（含 P1–P3 实施决策）  
**基线日期：** 2026-11-18  
**性质：** 架构决策与开发实施规格；非代码、非已实现产品声明  
**决策范围：** RCF-001—RCF-089 为显式架构基线，DFL 为缺省实施约定，TBD 编号及最新状态见 [§19](decisions.md#section-19)。  
**计划源码许可证：** `GPL-3.0-or-later`（RCF-089、TBD-002）  
**平台顺序：** Linux → Windows → Android → Apple 平台  
**渲染：** Linux 优先，轻量 RHI，P2/MVP 采用 Vulkan 1.3 后端。  
**阶段：** P1 最小内核和 P2 的交互、有限库存、本地存档及 Vulkan 1.3 离屏绘制已通过验证。用户确认此前的 P2 桌面键鼠验收通过。

> **实施状态与决策补记。** P1 内核、Vulkan 设备探测及 P2 平坦地形和权威交互已有验收记录。本轮确认的 P1–P3 实施方案登记在 [§19](decisions.md#section-19)。参见 [P1 决策](p1-decisions.md)、[P1 验收](p1-acceptance.md)和 [P2 首个里程碑](p2-milestone-1.md)。

<a name="section-0"></a>

## 0. 阅读说明、决策来源与状态

这组规格统一记录了 RustCraft 已结束的 89 轮需求访谈。发现跨模块矛盾时，应先核对[系统不变量](#section-1-3)、[决策登记](decisions.md)和[依赖边界](#section-3-4)，再进行受控修订。

<a name="section-0-1"></a>

### 0.1 决策来源标记（必须保留至所有正式版本）

| 标记 | 定义 | 对本版的应用 |
|---|---|---|
| **EXPLICIT** | 用户曾对具体问题显式作出选择 | RCF-001—RCF-089 全部是 EXPLICIT；RCF-089 的 GPLv3 也是明确选择 |
| **DEFAULT** | 用户授权将重复性的同类问题默认按「分层、版本化、可替换、精简 MVP」策略处理，但未逐题批准具体细节 | DFL-001—DFL-020；它们是**可覆写的设计提案**，不冒充 RCF 显式选择 |
| **TBD** | 尚待用户选择或提供参数的实现、合规与产品决策 | [§19](decisions.md#section-19) 保留 TBD-001—TBD-025 作为追踪编号，并记录各项确认状态与剩余问题 |

**状态不是实施完成度。** EXPLICIT 表示选择已确认，不表示功能已编码、测试或达到兼容。DEFAULT 表示架构设计采用授权的默认原则，也不表示用户逐题批准。TBD 不得在实现中变成无声明的事实标准。

<a name="section-0-2"></a>

### 0.2 文档规范级别与变更控制

- **MUST / 必须：** 硬性约束，除非变更 RCF 基线，否则不可违反。
- **SHOULD / 应当：** 强烈建议，允许有记录的例外。
- **MAY / 可以：** 设计留出的实现选择。
- 任何 RCF 变更须记录：变更动机、受影响模块、兼容性风险、存档/网络/规则迁移、测试和决策来源；不得默默改变历史选择。
- DEFAULT 事项可通过明确记录变成已采纳的项目约定；**不得直接改写为 EXPLICIT**。TBD 在真正决定后独立更新状态。

<a name="section-0-3"></a>

### 0.3 交付状态

这组规格记录架构、实施决策与阶段验收规范。仓库已提供 P1 内核、图形探测入口及 P2 无窗口交互入口。兼容性、性能、平台支持与发行合规结论以对应的验证记录为准。

---

<a name="section-1"></a>

## 1. 产品定位与明确目标

<a name="section-1-1"></a>

### 1.1 愿景

RustCraft 是**独立的 Rust 体素沙盒游戏引擎及客户端/权威模拟平台**。长期可通过模块化语义适配实现 Minecraft Java Edition、Bedrock Edition 不同版本的行为与资源兼容，并允许合理组合世界生成、方块/红石、实体 AI、战斗、物理、界面等规则，同时保留完全原创资产和原创玩法的独立运行路径。

并非目标是：在 MVP 阶段复制某一完整 Minecraft 版本；直接运行所有 Java Mod/JAR；立刻构建通用沙箱、云服务器或跨维度无缝真实物理；在没有证据时对外宣传「100% 兼容」。

<a name="section-1-2"></a>

### 1.2 两条长期产品路径

1. **Independent Sandbox：** 原创方块、世界生成、玩家控制、模块化规则与资源；不依赖任何 Minecraft 官方资产即可启动。
2. **Compatibility Presets：** 在获得合法资源访问条件、测试证据和必要适配实现后，为选定 Java/Bedrock 版本提供特定的规则/存档/资源/协议兼容能力。不同维度和能力域可依显式约束混搭版本化实现。

**MVP 采用第一条路径，第二条路径预留架构而不承诺功能覆盖率。** 首个行为差分兼容目标为 Java Edition 1.20.1（TBD-001）。

<a name="section-1-3"></a>

### 1.3 不可妥协的系统不变量

1. 权威世界状态仅经确定性、受控操作提交；客户端显示或本地预测不是权威。
2. 渲染、网格、资源、网络、模组 API 和持久化格式不直接定义游戏行为语义。
3. 游戏规则可版本化并可声明依赖与冲突；不以隐式加载顺序决定结果。
4. 规则兼容需证据和验证矩阵支撑；原版「特性/漏洞」是分项行为规格，不能用统一 `exploit` 布尔值掩盖。
5. 维度视觉可见、物理可达、可投射、可传送、可共享声音/光照是不同能力。
6. Minecraft 资产授权、账号认证、GPLv3 引擎版权及第三方 Mod 授权独立审查。
7. 原生模块能力声明不等同于内存沙箱；Stable 前不开发专门反作弊系统（RCF-060）。
8. 后台任务、缓存命中、随机数和线程调度的偶然性不应改变已规定的可观察模拟结果。
9. 初期不建立通用流体、气体、能量、热量、压力等工业资源域，即使是占位实现（RCF-029）。
10. RustCraft 无须在 MVP 实现完整生存玩法、在线多人、完整 Java/Bedrock 行为或第三方动态代码加载。

---

<a name="section-2"></a>

## 2. MVP 交付边界与验收契约

<a name="section-2-1"></a>

### 2.1 MVP 必须可玩的垂直切片

- 单进程集成的**服务端权威核心**与本地客户端，通过进程内消息通道交互；单 World Instance、单 Dimension Instance。
- Linux 优先的窗口、输入与基础 GUI。Vulkan 1.3 图形后端、轻量 RHI 与基础有序渲染 Pass，渲染循环与固定模拟 Tick 相互独立（[§9.1](client.md#section-9-1)）。
- 可移动的第一人称玩家、鼠标观察、重力与方块碰撞；创造模式风格的快速放置/破坏，服务端验证目标。
- 基础方块注册与 BlockState Schema，Uniform + Palette/Bit-Packed Section；Chunk/Section 寻址；简单实体注册和组件。
- 可替换的初级世界生成器；世界种子可复现；简单群系定义、少量基础地物（如树/矿脉，可作为扩展验收）。
- 玩家快捷栏/简易物品栏、基础 ItemStack 与槽位操作；少量简单配方接口测试（可以不提供完整工作台 GUI）。
- 静态链接 Rust 内建模组的注册、初始化、依赖解析、Manifest/Lockfile、有限配置 Schema；冻结注册表。
- Chunk 基础 Ticket 加载/卸载、简单后台任务、基础网格生成（方块面剔除和可合并面，特殊几何另设策略）、版本化结果校验。
- 原生世界保存/加载、版本字段、必要错误诊断、基本一致性检查；安全停止后完整备份及独立目录恢复能力属于可交付的最小恢复方案。
- 基础 HUD、暂停/设置、键鼠输入、简单音效/音乐与粒子、基础日夜天空、必要的本地反馈。
- 核心单元/集成/确定性/保存回归测试和最小持续集成；结构化日志、Tick 耗时、基本故障分类。

<a name="section-2-2"></a>

### 2.2 MVP 不做清单（不可因「扩展接口」而偷渡实施）

完整原版版本行为、完整生存/饥饿/经验、复杂怪物 AI 和自然刷怪、复杂附魔/药水/装备耐久、全面爆炸/火焰、动态第三方插件执行、WASM/JVM、Forge/Fabric 模组直接运行、Java/Bedrock 在线协议网关、远程互联网联机、分布式服务器、复杂跨维度连续物理、跨维度多视图完整效果、通用流体/能量/热模拟、实时 RT/GI、复杂骨骼动画、GPU Mesh Shader、多世界 Server Host、后台自动增量备份、完整 Replay/Time Travel、专门反作弊（直到首次 Stable 发布前均禁止）。

<a name="section-2-3"></a>

### 2.3 MVP 完成的可测试定义

| 类别 | 最低验收规则（方法） | 验收性质 |
|---|---|---|
| 启动 | Linux 上能建立窗口、创建 Vulkan 设备、进入可操作世界并安全退出 | 功能 |
| 基本交互 | 放置/破坏由权威模拟产生；重复输入与过期会话不产生不合法重复状态修改 | 正确性 |
| 世界持久化 | 保存、退出、重新加载后可重现玩家位置、基础区块、物品栏和必要注册映射 | 正确性 |
| 确定性 | 固定种子、配置、版本、输入序列下指定场景的权威状态校验一致 | 语义 |
| 版本检查 | 延迟完成的旧区块 Mesh/存储衍生任务不覆盖较新的源版本 | 并发安全 |
| 模组 | 内建模组按依赖关系注册；冲突/缺失依赖有可解释失败；锁定信息可保存 | 扩展性 |
| 错误恢复 | 失效配置、缺失依赖、损坏存档不能静默以不一致状态继续运行 | 健壮性 |
| 备份 | 安全检查点完整备份，恢复至独立目录并完成校验，不默认覆盖原世界 | 数据安全 |
| 验证记录 | 每条 MVP 必测能力关联测试用例和故障诊断输出 | 可追踪性 |

模拟逻辑步长为 50 ms（20 Hz）。实际负载上限、默认视距、参考硬件和内存、GPU 帧时间预算按 [§14.3](performance.md#section-14-3) 的 P3 性能基线确定。

---

<a name="section-3"></a>

## 3. 总体系统分层与模块边界

<a name="section-3-1"></a>

### 3.1 逻辑分层图

```text
  ┌──────────────────────── Client Shell / Input / UI ────────────────────────┐
  │ Client Presentation  →  Render Graph / Mesh / RHI / Vulkan              │
  │ Audio / VFX / Animation / Asset Providers / Visual Prediction          │
  └───────────────────────────┬──────────────────────────────────────────────┘
                              │ Semantic Commands / Snapshots / Deltas
  ┌───────────────────────────▼──────────────────────────────────────────────┐
  │ Session / Capability Negotiation / Transport-independent Message Model │
  └───────────────────────────┬──────────────────────────────────────────────┘
                              │ Authoritative command boundary
  ┌───────────────────────────▼──────────────────────────────────────────────┐
  │ Simulation Core: Tick / Stages / Events / Mutations / RNG / Scheduler   │
  │ Rule Resolver: Versioned Presets / Capabilities / Implementations       │
  │ World Container → Dimension Instances → Chunk/Section + Entity ECS      │
  │ Gameplay Modules: Blocks, Inventory, Physics, Interaction, Worldgen ...  │
  └───────────────┬───────────────────────────────────┬──────────────────────┘
                  │ snapshots / change versions       │ lifecycle / permissions
  ┌───────────────▼────────────────┐   ┌──────────────▼─────────────────────┐
  │ Persistence / Region / Backup  │   │ Mod Registry / Manifest / Policy   │
  └────────────────────────────────┘   └────────────────────────────────────┘
```

上图描述逻辑职责。以下 DEFAULT 架构分组对应的实际 Cargo 工作区边界见 [§3.4](#section-3-4)。

<a name="section-3-2"></a>

### 3.2 逻辑模块建议（DEFAULT）

| 分组 | 负责 | 不得承担 |
|---|---|---|
| Platform & Host | 窗口、设备、输入、生命周期、配置入口 | 决定战斗/红石语义 |
| Authority Core | Tick、命令、调度、状态事务、确定性 RNG | 读取 Vulkan 设备、任意文件路径 |
| World Model | Chunk/Section/Palette、BlockState、Entity、Dimension | 直接耦合某个原版协议 |
| Gameplay Rules | 方块通知、物理、AI、战斗、库存、交互等可替换实现 | 绕过事务入口 |
| Mod & Policy | 生命周期、能力/权限、依赖解析、规则预设、Lockfile | 暗示原生不可信代码已被沙箱化 |
| Persistence | 原生存档、版本迁移、写回、恢复/备份 | 把运行时 ECS 句柄写成持久 ID |
| Network Semantics | 会话、命令、复制视图、同步策略、协议适配 | 让某个网络协议成为内部模型 |
| Client Render | 资源、Mesh、RHI、Vulkan、多视图、后处理 | 修改权威方块状态 |
| Client UX | HUD、输入映射、GUI、音频、粒子、动画 | 决定权威库存操作 |
| Diagnostics & Tests | 结构化诊断、事件/版本追踪、兼容性矩阵 | 擅自改变权威规则 |

<a name="section-3-3"></a>

### 3.3 运行角色

**Client：** 采集输入、解释表现、可做受约束预测、发起命令、消费权威状态；可以与集成服务端同进程但仍经语义边界隔离。  
**Integrated Server：** 唯一模拟权威和存档提交源；MVP 唯一部署模式。  
**Dedicated Server / LAN Host：** 长期通过同一模拟核心和不同 Host/Transport 实现；非独立规则引擎。  
**Compatibility Gateways：** 未来 Java/Bedrock 协议适配器，不改变内部权威消息语义。  
**Mod/Script Runtimes：** 当前只静态链接可信 Rust 内建模组；WASM、原生动态库、JVM/JAR、JS 脚本在各自经过验证的后续阶段确定。

<a name="section-3-4"></a>

### 3.4 Cargo 工作区与构建边界（TBD-004）

采用单一虚拟 Cargo Workspace。主要 crate 边界为权威模拟、世界数据、模组 API、持久化、图形后端及运行入口，完整目标约 16 个包。内部优先使用 Rust module，出现独立依赖、实现替换、测试或编译边界时再拆分 crate。公开 API 受控，依赖保持单向。

| P1 crate | 职责 | 工作区依赖 |
|---|---|---|
| `rustcraft-world` | 世界坐标、区块与 Section 数据 | — |
| `rustcraft-mod-api` | 注册表、模组依赖与组件契约 | `rustcraft-world` |
| `rustcraft-authority` | 权威 Tick、事务和 ECS 适配 | `rustcraft-world`、`rustcraft-mod-api` |
| `rustcraft-vulkan` | Linux Vulkan 窗口与图形后端 | — |
| `rustcraft-host` | 进程内消息及服务端、客户端入口 | `rustcraft-authority`、可选 `rustcraft-vulkan` |

服务端可独立构建，客户端通过 `client` feature 引入图形后端。P2 新增 `rustcraft-persistence`，依赖权威核心的检查点接口和世界数据类型，由 `rustcraft-host` 调用。

`rustcraft-scene` 持有资源读入、CPU 区块网格和绘制数据，依赖世界数据类型。客户端入口和 Vulkan 后端共用该 crate，默认无图形服务端构建保留独立依赖链。

---

<a name="section-22"></a>

## 22. 术语和语义边界

| 术语 | 含义 |
|---|---|
| Authoritative State | 服务端控制的权威游戏世界状态，客户端渲染副本不是权威 |
| World Container | 一个世界内多个 Dimension Instance 的逻辑容器，区别于管理多个世界的 Server Host |
| Dimension Relationship Graph | 维度间空间、视觉、传输和规则能力关系，不是 GPU Render Graph |
| Capability / Implementation / Preset | 能力契约／具体版本化实现／合法能力组合的声明式规则预设 |
| Resolved Runtime Manifest | 最终解析的模组、版本、规则、配置与来源；用于重现启动和存档语义 |
| BlockState / Runtime ID / Persistent ID | 语义状态／当前进程快速索引／稳定可迁移的存档标识 |
| Mutation Request / Commit | 提出修改的合法意图／满足条件并真正改变权威状态的操作 |
| Read View / Snapshot | 带一致性范围、源版本与生命周期的只读数据，不保证未来可以提交基于它的结果 |
| Ticket | 区块加载、生成、模拟或渲染可用性等能力需求，不总是启动 Tick |
| EXPLICIT / DEFAULT / TBD | 用户逐题确认／授权原则下推定／尚未决策 |
| Strict Ruleset | 以独立行为规范和不变量限制异常副作用的规则预设，不宣称绝对零漏洞 |
| Verified-Compatible | 特定版本/功能/测试场景有可追溯验证支持，非全量兼容保证 |
