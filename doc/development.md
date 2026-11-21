# 开发路线与质量门禁

[文档索引](README.md)

<a name="section-15"></a>

## 15. 测试、净室兼容与阶段质量门禁

<a name="section-15-1"></a>

### 15.1 测试层次

| 层次 | 主要测试对象 | 首次要求 |
|---|---|---|
| 单元 | Section Palette、注册表、种子派生、Schema、权限与依赖解析 | MVP |
| 集成 | Chunk 生命周期、玩家放置/破坏、库存事务、保存/加载 | MVP |
| 确定性 | 固定输入/种子/规则下的 Tick 序列与状态校验 | MVP |
| 异步安全 | 旧 Mesh/存储结果拒绝、任务取消、边界失效 | MVP |
| 故障与恢复 | 断电模拟、写回失败、缺模组、存档损坏、备份验证 | MVP 基础 |
| 版本化语义 | 原版红石/方块更新、物理、战斗、RNG 等差分行为 | 兼容阶段 |
| 兼容矩阵 | 版本 × 规则预设 × 模组集 × 运行环境 | 分阶段 |
| 图形 | 跨平台 Vulkan、资源生命周期、Mesh 一致性、多视图 | 分阶段 |
| 性能回归 | Tick、内存、磁盘、网格、GPU、协议复制 | MVP 基础、持续增强 |
| 安全 | 权限/认证/资源限制、文件来源及许可审查 | 对应功能上线前 |

P3 存档恢复重放使用种子 42 和视距半径 1，放置方块后在 Tick 边界关闭玩家会话并保存。内存中继续运行的世界与从磁盘恢复的世界分别建立玩家会话，逐 Tick 执行同一组请求。测试比较玩家、实体、库存、活动区块坐标与版本、体素内容、事务回执及音效事件，并核对结束时的完整检查点。路线覆盖新区块生成、已编辑区块退回驻留并重新激活、方块回收和过期请求拒绝。恢复后的调色板以体素内容核对，会话标识按各分支绑定。参见 [重放用例](../crates/persistence/tests/replay.rs)。

在仓库根目录运行专项验收。Linux 子进程测试覆盖 `CURRENT` 持久化前后的强制终止。集成测试覆盖遗留目录、并发写入、Region 缺失或损坏以及独立备份恢复。

```sh
cargo test --locked -p rustcraft-authority --tests &&
cargo test --locked -p rustcraft-persistence --lib --test replay --test recovery --test random_streams --test scheduled_tasks &&
cargo test --locked -p rustcraft-host --no-default-features --test checkpoint_recovery --test task_demo --test task_commands
```

<a name="section-15-2"></a>

### 15.2 净室与证据管理

公开文档、合法原版黑盒实验、版本化行为规格、差分测试与独立 Rust 实现之间必须留有证据来源。对涉及反编译或高版权风险的部件采取更严格的隔离与审查；净室方法本身不构成法律保证。测试记录应说明：被测试版本、配置、环境、输入、预期/实际、证据来源和验证状态。不能因实现一个同名 API 即宣传全面兼容。

<a name="section-15-3"></a>

### 15.3 差分测试典型矩阵

- 同一个区块/结构种子在给定原版版本下的可复现结果。
- 红石更新顺序、特定原版历史特性、Strict 不变量，逐条独立测试。
- 投射物同 Tick 命中顺序和跨维度关系能力。
- 库存/容器并发、合成输入变化及重发去重。
- 随机序列、持续任务保存/恢复、状态效果和爆炸/火焰规则。
- 旧存档格式升级、未知模组组件保留、跨维度迁移崩溃恢复。

<a name="section-15-4"></a>

### 15.4 Linux 验证与 CI

主验收机为 Legion Y7000P IRX9，运行 Gentoo Linux x86-64、KDE Plasma / KWin Wayland，使用 Intel Raptor Lake-S 核显。补测同机 NVIDIA 独显的窗口流送，分别记录两块 GPU 的 Vulkan 驱动与版本。X11 暂不支持（TBD-015）。

基础 Linux CI 使用托管的 Ubuntu 24.04 Runner，在 PR、`main` 更新或手动触发时运行格式与文档检查、Clippy、全功能工作区测试和独立服务端演示。演示需要输出 `event=p2_demo_complete` 并以零退出。参见 [Linux CI 工作流](../.github/workflows/linux-ci.yml)与 [GitHub Runner 文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)。

当前阶段以本地命令和 Gentoo 实机为验收入口。远端 Vulkan 首帧验收延期到云服务器确定后（TBD-024）。恢复后在 `main` 上用带 `rustcraft-gpu` 标签的 Runner 执行，保存硬件 GPU 日志、首帧事件和零退出结果。参见 [首帧工作流](../.github/workflows/vulkan-first-frame.yml)与 [P3 验收清单](p3-acceptance.md#github-与-gentoo-剩余验收)。

---

<a name="section-16"></a>

## 16. 分阶段开发路线与关键路径

路线表达先后依赖和可验收出口，不表示日历承诺。后期功能可并行预研，但不得在前阶段注入未明确的运行时依赖。

| 阶段 | 交付主题 | 出口门槛 | 不应提前实现 |
|---|---|---|---|
| **P0 文档基线** | RCF 决策登记、模块边界、MVP 契约、风险与 TBD 清单 | 本文档完整、无伪造显式决策 | 源代码与实际兼容宣称 |
| **P1 最小内核** | 平台 Linux/Vulkan、Tick、基础 Registry、Section、Chunk、内建模组依赖与本地消息 | 世界能初始化、固定 Tick、基本诊断、可重演测试 | 完整原版内容 |
| **P2 MVP 垂直切片** | 世界生成、玩家控制、碰撞、放置/破坏、简易库存、Mesh、存档、HUD、音效 | 完成第 2.3 节 MVP 验收 | JVM、复杂资源系统、远程多人 |
| **P3 稳定性与规则基础** | 版本化规则接口完善、备份/迁移、权限、调度/随机语义、测试矩阵、性能基线 | 可复现的保存/恢复，本地任务管理与重放通过，Gentoo Intel 核显五档窗口日志归档 | 高开销复杂功能无序加入 |
| **P4 原版/模组兼容增量** | 选定 Java 兼容目标、数据与资源转换、红石/技术生存专项差分、后续 Forge/Fabric 语义桥 | 逐能力发布 Verified-Compatible 范围 | 一次性承诺完整 JAR 兼容 |
| **P5 多维度/联机/高级表现** | 维度关系图实例、跨维度视觉/投射物、网络 Gateway、Render Graph、高级图形 | 用明确场景测试跨维度一致性和能力隔离 | 强制所有维度全量模拟 |
| **P6 首次 Stable 门禁** | 可复现实测性能、存档安全、授权合规、发行及兼容边界 | 全部 Stable 发布门禁经定义与验证 | **Stable 发布前不得开发专门反作弊** |
| **P7 Stable 之后** | 可选反作弊、第三方隔离执行载体、动态热替换、大规模部署 | 各项独立需求与测试 | 假设所有未来模块必做 |

<a name="section-16-1"></a>

### 16.1 前置依赖链

**BlockState/Section → Chunk & World Snapshot → Authority Mutation/Event → Player Interaction → Native Save → Meshing/Render → MVP 游戏闭环。** 规则 Manifest/内建模组生命周期必须在世界初始化前完成。网络进程内适配可在 Authority Interface 确定后并行实施。高级模组兼容不能先于语义接口和验证基础设施。

<a name="section-16-2"></a>

### 16.2 首个 Stable 发布的负面约束

Stable 不是「全部 Minecraft 都兼容」的别名。每次发布声明游戏模式、支持平台、实验性功能、验证通过的规则组合、兼容范围和存档升级保障。特别检查 RCF-060 在 Stable 之前未被范围蔓延破坏。

---

<a name="section-17"></a>

## 17. GPLv3、第三方资产与合规边界

<a name="section-17-1"></a>

### 17.1 自有源码许可与贡献政策

RustCraft 自有源码采用 GPLv3 或其后续版本，SPDX 标识为 `GPL-3.0-or-later`（RCF-089、TBD-002）。发布时补全版权声明、许可证全文和必要的源码提供方式，检查第三方链接依赖及可再分发性。参见 [SPDX 许可证条目](https://spdx.org/licenses/GPL-3.0-or-later.html)。

后续外部贡献采用 DCO 1.1 签署政策（TBD-003）。现有源码版权声明的具体署名待用户提供。参见 [Developer Certificate of Origin 1.1](https://developercertificate.org/)。

<a name="section-17-2"></a>

### 17.2 许可证、资产、商标的不同维度

| 对象 | 本文档原则 |
|---|---|
| RustCraft 自有引擎及自有游戏源码 | `GPL-3.0-or-later`，外部贡献采用 DCO 1.1 |
| 原创美术、音频、字体、文本 | 单独明确作品许可；不因源码 GPLv3 自动推断第三方素材可分发 |
| 第三方 Rust crates/原生链接库 | 发布前依赖许可证兼容清单、通知义务与构建检查 |
| 第三方 Mod/JAR/脚本 | 根据其作者许可及部署/链接方式审查；不假定全部自动属于 RustCraft 的 GPLv3 |
| Minecraft Java/Bedrock 资产 | 仅在合法来源、用户权利及适用许可允许下读取/转换/缓存；未经授权不捆绑分发 |
| Minecraft 名称/商标/徽标 | 不冒充 Mojang/Microsoft 官方或获授权产品，遵守相关品牌与使用规则 |
| Microsoft 登录令牌 | 标准认证流程、受保护存储、日志脱敏，不直接采集密码 |

<a name="section-17-3"></a>

### 17.3 开发与发行前的合规门禁

- 独立原生资产集必须使无 Minecraft 安装的用户也能运行 RustCraft 原创沙盒。
- P1–P3 随包资源以原创资产为主。第三方素材逐项确认许可可覆盖拟用途，并记录来源、许可和署名。P4 的 Java 1.20.1 本地参考资源来自用户的 Prism Launcher 安装，放在 Git 忽略的 `local-resources/minecraft-java/1.20.1/`。`client-assets/` 保存客户端包中的 `assets/`，`indexed-assets/` 保存资源索引 5 的逻辑路径，`extraction.json` 记录提取数量。正式资源导入在兼容阶段决定（TBD-022）。
- 使用 `cargo-deny` 检查锁文件所解析的 Rust crate 许可证，另行记录原生链接库及分发义务。插件和嵌入脚本载体按各自立项范围审查（TBD-023）。参见 [cargo-deny 许可证检查](https://embarkstudios.github.io/cargo-deny/checks/licenses/index.html)。
- 使用公开信息及合法黑盒实验建立版本化行为规格和测试，隔离高风险来源代码；工程审计和法律审查独立进行。
- 所有可发布二进制/源码/资源构件必须有软件物料清单、许可清单及第三方声明。
- Java/Bedrock 协议、正版身份校验和用户本地资产读取分别验证，不能以「合法登录」推断可再分发权。
- 若 GPLv3 链接边界、插件分发、游戏名称/商标或官方资源条款发生冲突，应在发布前升级 TBD 法律审查，而不是静默放宽约束。

**参考法规/政策原文（本文件未提供法律意见）：**

- GNU GPLv3 正文：https://www.gnu.org/licenses/gpl-3.0.en.html
- SPDX GPL-3.0-only：https://spdx.org/licenses/GPL-3.0-only.html
- SPDX GPL-3.0-or-later：https://spdx.org/licenses/GPL-3.0-or-later.html
- Minecraft EULA：https://www.minecraft.net/en-us/eula
- Minecraft Usage Guidelines：https://www.minecraft.net/en-us/usage-guidelines

---

<a name="section-21"></a>

## 21. 模块交付与测试追踪矩阵

| 工作流 | 入口/依赖 | 数据与权威结果 | 主要风险 | 首次验证阶段 |
|---|---|---|---|---|
| 启动世界 | RCF-010/037/039/061/064 | Resolved Manifest → 冻结注册表 → World Instance | 规则冲突/缺失 Mod | P1 |
| 加载/生成区块 | RCF-008/030/032/069/070 | 只读任务 → 版本验证 → 激活 Chunk | 旧任务覆盖/边界不一致 | P1/P2 |
| 挖掘/放置 | RCF-055/074/077/080 | Session → 射线验证 → Mutation → Delta | 伪造输入/重复变更 | P2 |
| 玩家库存 | RCF-028/071/081 | 槽位操作 → 原子提交 → 复制 | 并发复制/鼠标携带丢失 | P2 |
| 保存与恢复 | RCF-022/065/066/067/070 | 一致视图 → 写回提交 → 校验恢复 | 不完整落盘/依赖漂移 | P2/P3 |
| 规则版本混搭 | RCF-010/023/024/041 | 能力解析 → 规则测试 → Verified 范围 | 算法/调度冲突 | P3/P4 |
| 跨维度投射物 | RCF-009/017/034/071/086 | 空间关系 → 命中 → 迁移 → 同步 | 重复/丢失实体、递归视图 | P5 |
| 模组/脚本兼容 | RCF-020/021/038/052 | 证据 → 语义适配 → 权限 → 差分 | 版权/原生代码隔离不足 | P4+ |
| Open Source 发行 | RCF-018/019/020/039/089 | 源码/依赖/原创资产清单及合法发布 | GPL/资产/商标冲突 | 发行前 |

---

<a name="section-23"></a>

## 23. 当前实施进度与后续验收

[§20](rcf-decisions.md#section-20) 记录 89 项 RCF 架构基线，[§18](decisions.md#section-18) 记录 20 项 DFL 缺省约定。[§19](decisions.md#section-19) 给出 P1–P3 已确认的实施方案及剩余待补项。

[P3 验收清单](p3-acceptance.md)汇总本地复验、GitHub CI、专用 GPU Runner 和 Gentoo 实机的剩余证据。

P1 内核及 P2 无窗口交互、有限库存和本地存档通过验证。完整检查点备份和独立目录恢复见 [§13.5](persistence.md#section-13-5)。P2 已接入原生资源打包、区块网格、第一人称客户端和快捷栏。用户确认桌面键鼠、Gentoo 原生 PipeWire 音效、F3 HUD 及 F3+G 区块边界实机验收通过。自动恢复捕获、方块描边和准星短闪已通过自动化验证，实机回归以桌面记录为准。

P3 已完成 Gentoo 无窗口区块、Intel 核显离屏和真实窗口区块流送采样。默认视距与高视距各三次窗口复测均通过首版补齐门槛，结果和检查命令见 [§14.3](performance.md#section-14-3)。参见 [P1 验收](p1-acceptance.md)与 [P2 桌面键鼠验收](p2-desktop-acceptance.md)。

P3 已接入原生规则版本锁定、独立随机流和世界级延迟任务。存档测试覆盖旧世界读取、备份、离线迁移、继续探索、飞行恢复及方块与库存事务。[专项验收](#section-15-1)还覆盖权限矩阵、存档重放、随机流续接和延迟任务恢复。操作命令见 [§13.5–13.7](persistence.md#section-13-5)。

Java 1.20.1 红石的长期目标、P0–P2 达成状态与剩余差距见[红石长期目标与达成情况](java-redstone-goal.md)。P0 核心元件已有实现和选定原版差分；P1 未开始，P2 仅局部完成，最终验收尚未达成，不能对外宣称完整 Java 1.20.1 红石兼容。

双击空格切换飞行、下降触地恢复行走及 1–16 视距已接入。飞行与存档回归见 [§9.7](client.md#section-9-7) 和 [§13.5](persistence.md#section-13-5)，五档窗口复测命令见 [§14.3](performance.md#section-14-3)。
