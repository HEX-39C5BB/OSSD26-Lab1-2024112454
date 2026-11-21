# 模组、脚本与联网

[文档索引](README.md)

<a name="section-11"></a>

## 11. 模组、脚本、资源/数据兼容与安全

<a name="section-11-1"></a>

### 11.1 模组生命周期

模组经历发现、Manifest 解析、能力授权、注册、初始化、激活、暂停与退出。注册表原则上在世界激活前冻结。资源热重载、客户端配置更新及逻辑代码热替换分属不同生命周期；后者需安全 Tick 同步点、任务清理、数据迁移和失败恢复，MVP 不承担动态卸载。

<a name="section-11-2"></a>

### 11.2 执行载体与信任模型

长期统一模组语义 API 不绑定运行语言或 ABI；MVP 为静态 Rust 内建模组。未来 WASM、原生动态库、JVM/JAR、Script Runtime 分别评估可信性、性能和兼容范围。可信核心、受信扩展与不受信第三方需要权限声明与能力边界；**原生代码的 API 权限声明不是隔离任意内存访问的沙箱**。网络、文件、Shader、世界修改和管理权限应分别授权，资源配额不等于安全隔离。

<a name="section-11-3"></a>

### 11.3 兼容阶梯

1. 原版资源和数据映射/转换。
2. Java/Bedrock 游戏行为的版本化规则及测试。
3. Forge/NeoForge/Fabric 注册表、事件和 API 语义适配。
4. 执行载体研究（例如 JVM/JAR）和更复杂的 Mixin/字节码转换兼容。
5. 整合包逐包测试并声明经过验证的范围。

上述层级不自动推导上层能力。Minecraft Java 客户端和 Bedrock 客户端协议通过独立网关适配服务端内部消息模型；资源转换与游戏协议兼容分别验证。

<a name="section-11-4"></a>

### 11.4 脚本路线

长期优先提供 JavaScript 执行入口，支持 TypeScript 编译到 JavaScript 的开发流程，以 Bedrock Script API（特别是 `@minecraft/server`，随后 `@minecraft/server-ui`）为重要语义兼容目标。脚本语言引擎版本、Script API 模块版本与游戏规则版本分别解析。脚本事件、Tick 任务、实体/世界操作经权限与权威事务校验；异常隔离、资源预算和真正沙箱能力需要实际执行载体保证。MVP 无 JS 运行时要求。

<a name="section-11-5"></a>

### 11.5 命令与自动完成

统一版本化命令树，类型化参数和权限验证。所有版本的 RustCraft 客户端均使用现代 Shell 式命令编辑体验，包括补全、内联建议、历史、语法高亮和模糊匹配等 UX 能力；命令解析及游戏效果由对应原版/自定义规则语义决定。游戏命令与 Server Host 管理命令共享必要解析基础，但权限作用域不得混用。

P4 本地命令行以 `/` 在左下角打开。`tp` 支持本地玩家与相对坐标，`give` 支持 `@s` 和已注册物品，`gamemode` 切换创造或生存。游戏命令在权威 Tick 校验并提交。`freecam` 切换本地摄像机，`redstone` 控制 Debug 或 `dev-tools` 构建的逐 Tick 视图。参见 [P4 命令示例](p4-compatibility.md#构建并运行)。

<a name="section-11-6"></a>

### 11.6 模组包与来源

开放的模组包 Manifest/Lockfile 记录稳定 ID、能力、依赖、版本约束、来源、哈希和必要签名/许可元数据；仓库 Provider 可支持本地、第三方和未来官方仓库，不强制中心化。来源完整性和签名不等于代码无恶意，也不自动解决 GPLv3/资产许可问题。MVP 只有内建模组 Manifest/Lockfile 基础能力。

---

<a name="section-12"></a>

## 12. 联机、复制、客户端预测与服务器控制面

<a name="section-12-1"></a>

### 12.1 语义消息模型

定义 Command、Authoritative Event、Snapshot、Delta、Ack/Result 以及 Session/Capability 协商，而非将 Java 或 Bedrock 原版网络包直接作为模拟内部结构。Transport 可按语义支持可靠有序、可靠无序、不可靠/过期消息；具体能力取决于后端和协商结果。MVP 使用进程内通道。

<a name="section-12-2"></a>

### 12.2 连接、身份及规则一致性

连接生命周期明确包含传输、协议/能力协商、认证、规则兼容、世界准备、玩家激活、断开/恢复。必须检查游戏中**实际生效的权威规则语义**、关键 Manifest/Lockfile 约束及必要客户端能力，不强制所有仅客户端表现的模组文件完全一致。不兼容时给出可解释诊断并拒绝接入。过期 Session 和重放的世界操作不得被接受。

<a name="section-12-3"></a>

### 12.3 Interest & Replication

RCF-057 将玩家/客户端的关注区域、订阅预算、世界状态快照、版本化基线、增量复制、权限过滤和重连重同步分开。兴趣管理 ≠ Chunk 物理加载 Ticket；远端维度视觉订阅不自动激活远端模拟。网络可靠投递不能保证操作恰好执行一次，关键库存/世界操作需操作标识及去重或幂等保证。

<a name="section-12-4"></a>

### 12.4 Prediction & Reconciliation

RCF-058 将渲染插值、客户端预测、输入顺序/重放、服务器校正和视觉状态恢复分开。预测只在声明支持的能力范围内执行。跨维度迁移、传送和重大规则更换可构成位置/时间不连续点，必须显式重建预测状态。MVP 只需要简单视觉插值和命令结果校正，不承诺完整预测回滚。

<a name="section-12-5"></a>

### 12.5 延迟补偿

RCF-059 在长期架构中允许有限权威历史状态窗口、输入时间映射、版本化命中判定和历史查询预算。客户端时间戳不可信；历史数据仅用于合法性判定，结果在当前权威事务提交，不重写过去世界。MVP 集成服务端无须历史命中回溯。

<a name="section-12-6"></a>

### 12.6 反作弊的特别硬限制

**RCF-060（EXPLICIT）：首次 Stable 稳定版本发布以前，不开发专门反作弊模块。** 具体包括异常行为检测、作弊评分、自动惩罚、专门反作弊日志/执法策略。Stable 前只实施游戏规则生效语义/兼容性协商与校验，以及正常必需的会话认证、协议解析和权威状态提交。此条不能用「安全」或「服务端校验」名义扩大为隐性反作弊项目。Stable 之后如要开发，再另立架构和发布计划。

<a name="section-12-7"></a>

### 12.7 Server Host 与管理控制面

Server Host 与 World Instance Manager、World Container 和 Dimension Instance 分层；启动时先解析注册表、依赖、权限、存档及规则 Manifest 再激活。安全关闭在 Tick 同步点停止新请求、完成必要事务和持久化并释放资源。管理操作按游戏内、世界实例和 Host 进程级权限分域；MVP 仅状态查询、安全保存、正常退出与错误诊断，远程管理默认不开放。

P3 的权威事务按 Session 授权。`open_player_session` 为本地玩家建立唯一的控制会话，`open_local_session` 接收受信 Host 授予的管理能力。

| 会话或能力 | 可提交的权威操作 |
|---|---|
| 本地玩家会话 | `ControlPlayer`、`UseBlock`、`EditInventory` |
| Host `edit_blocks` | `SetBlock` |
| Host `manage_chunks` | `SetChunkActive` |
| Host `edit_entities` | `SpawnEntity`、`DespawnEntity`、`SetComponent` |

权威提交先检查 Session 和操作序号，再核对能力与操作前置条件。越权请求返回 `MissingCapability`，占用该操作序号。关闭会话后旧请求返回 `UnknownSession`，关闭玩家会话同时清除暂存输入。参见 [权限验收](../crates/authority/tests/p2.rs)。
