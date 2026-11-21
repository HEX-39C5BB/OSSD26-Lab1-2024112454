# 持久化、备份与恢复

[文档索引](README.md)

<a name="section-13"></a>

## 13. 持久化、存储后端、备份与恢复

<a name="section-13-1"></a>

### 13.1 原生存档设计

存档采用 RustCraft 原生格式，**持久化模型与运行时内存布局分离**。记录世界身份、规则 Manifest/Lockfile、生成器和配置版本、维度实例/关系、区块及实体组件、稳定资源标识、玩家档案及必要任务/RNG 状态。缺失模组数据尽可能保留为可识别的未知数据；Java/Bedrock 存档导入/导出由独立转换层实现，原文件保持不被修改，导出可能有损。

世界 Manifest 使用 JSON，区块和组件采用确定性 CBOR，各 Schema 独立记录版本。加载遇到未知主版本时停止并报告版本信息，缺失模组的组件数据尽量按原始记录保留。迁移向新目录输出，验证迁移结果后可启用新存档（TBD-011）。参见 [CBOR 确定性编码规范](https://www.rfc-editor.org/rfc/rfc8949.html#section-4.2)。

<a name="section-13-2"></a>

### 13.2 存储后端与写入

RCF-067 确定分层存储模型，覆盖逻辑对象与 Schema、区域索引、可替换本地后端、压缩策略、异步写回以及提交与恢复协议。P2 原生 Region 覆盖横向 `16×16` 个区块，区块记录独立索引并使用 CRC32C 校验，首版为未压缩编码。通过一致性检查点发布新版本（TBD-010）。

后台写回携带源版本，提交前校验版本并拒绝过期结果。内存事务与写回进度分开记录，存储提交协议保证崩溃后可定位一致性检查点。

<a name="section-13-3"></a>

### 13.3 备份模型

RCF-066 的应用层一致性检查点、Backup Manifest、完整/增量链、校验、恢复点索引、保留策略和恢复执行分离。多维度区块、实体、玩家和跨维度事务需在一条明确的逻辑恢复边界上形成可恢复点。文件系统快照不能自动保证应用一致性。MVP 只做安全停止/安全保存后的完整备份、基本校验和隔离目录恢复；在线增量/异地复制/连续时间点恢复后置。

<a name="section-13-4"></a>

### 13.4 故障域与恢复

任务、模组、系统、维度、World Instance、Server Host、客户端/图形后端区分逻辑故障域；任务失败可有界重试，但如果权威 Tick 状态无法证明一致，应安全停止受影响世界/实例，而不是跳过关键规则事件。MVP 静态原生模块无法宣称进程级内存故障隔离。恢复流程应先验证存档、模组依赖、注册表映射及校验，再经明确操作启用，不默认覆盖原世界。

`CURRENT` 决定检查点的发布边界。新代目录已经写入时，旧 `CURRENT` 仍指向旧代，校验和备份继续读取旧代。重试保存会分配新代号。两个写入者持有同一代号时，先完成提交的写入者发布新代，另一方收到代号已变化的错误。指向缺失目标的 `CURRENT` 符号链接会被报告为无效指针，避免将该目录作为新世界打开。

Region 缺失、截断或校验值不符时，加载会报告代号和 Region 文件名。校验失败的检查点无法发布为备份或迁移结果。已有有效备份可通过 `--restore BACKUP NEW_DIR` 恢复到独立目录。

Linux 进程中断测试使用实际保存路径准备新代，并在更新 `CURRENT` 前或其目录同步后发送 `SIGKILL`。重开后核对 `CURRENT` 和解码后的检查点，并验证写锁释放后可继续保存。参见 [P3 验收清单](p3-acceptance.md)。

<a name="section-13-5"></a>

### 13.5 P2 本地保存与继续运行

```sh
cargo run --locked -p rustcraft-host --bin rustcraft-server -- --world ./world --seed 42 --ticks 16 &&
cargo run --locked -p rustcraft-host --bin rustcraft-server -- --open-world ./world --ticks 5
```

`--world DIR` 可创建新世界或打开已有世界。新世界可通过 `--height MIN MAX_EXCLUSIVE` 指定高度。客户端和独立服务端均支持 `--open-world DIR`，用于继续已有存档或打开恢复后的目录。严格打开时要求 `CURRENT` 指向可加载的检查点。目录缺少已发布检查点或校验失败时会立即退出。两种启动选项互斥。正常运行结束后保存到新一代检查点，已发布的上一代保留。

检查点覆盖本地玩家位置、朝向、库存和驻留区块。区块以稳定方块标识及 Schema 版本建立调色板，非均匀 Section 保存 Little-endian `u16` 索引。超出首版 Schema 的实体或扩展组件会导致保存返回错误，现有检查点保留。

当前 `manifest.json` 使用 Schema 8。`rules` 保存各项原生规则的 `id` 和 `version`。`random_streams` 的记录 Schema 1 保存已提交随机流的作用域、系统标识和 64 位状态。`scheduled_tasks` 的记录 Schema 2 保存下一任务编号和待处理项的编号、到期 Tick 与操作参数。方块参数使用稳定名称与版本。记录 Schema 1 的待处理项继续触发到期事件。Schema 1–7 的 Manifest 仍可读取，缺少相应记录时从空随机流表或空任务队列开始。清单大小上限为 4 MiB，以容纳完整的有参数任务队列。未知版本、缺失必需记录或无效状态会导致校验失败。

Java 超平坦预设版本 2、3 的 `java_redstone` 使用记录 Schema 4。记录保存活塞动作、运动占位、各元件的计划刻，以及比较器已提交的非零输出。Schema 4 增加发射器和投掷器的九槽库存、掉落物的稳定身份、位置、速度、年龄和下一实体 ID。加载校验记录坐标、方块类型、堆叠及实体身份的有效性。旧记录 Schema 1–3 仍可读取。按钮释放、压力板重新检测及发射或吐出待办均保留原截止 GT。火把重开会清空近期熄灭次数，已烧毁火把的恢复刻保留。[比较器重开](java-redstone-verification.md#comparator-evidence) · [火把重开](java-redstone-verification.md#torch-evidence) · [容器和掉落物](java-redstone-verification.md#ejector-evidence)

V3（`--preset java-1.20.1-reference-v3`）按原版重开证据丢弃未执行的活塞 Block Event，运动记录保存上一 GT 的进度。恢复后首个活动 GT 等待方块实体就绪，再继续运动；保存操作不清空当前运行队列。V2 保留既有存档行为，两者通过预设版本区分 Schema 4 中运动字段的语义，旧世界不会自动升级。发射器和投掷器的计划刻及库存继续持久化。[独立进程重开实验](java-redstone-verification.md#qc-restart-evidence)

参考预设的红石粉连接和电平由区块调色板中的稳定方块标识保存。旧存档中只记录电平的粉末仍可读取，后续收到形状通知或被玩家切换时可写入完整连接。读档沿用保存的方块状态。[红石粉连接测试](java-redstone-verification.md#wire-shape-evidence)

玩家 CBOR Schema 2 保存飞行模式。Schema 3 增加 Java 超平坦预设的 36 槽背包和光标持有的堆叠。Schema 4 记录创造或生存模式，并按预设验证槽位数。Schema 1–3 仍可读取。缺少模式时，世界启动使用创造模式。显式 `--gamemode` 优先，关闭时保存实际模式。已有的原型 `redstone_check` 待办仍可运行。

将包含 `CURRENT` 的存档目录传给 `--verify-world`，即可在启动、备份或恢复前只读校验已发布的检查点。命令使用正常加载路径校验清单、规则锁定、记录校验值和恢复后的世界状态。成功时输出 `event=checkpoint_verified`，包含代号、Tick、种子、活动区块数和规则版本。缺少已发布检查点或校验失败时返回非零退出码。

```sh
cargo run --locked -p rustcraft-host --no-default-features --bin rustcraft-server -- --verify-world ./world
```

以下命令在 `target` 下创建独立演示目录，依次生成存档、校验、备份、恢复并继续运行。任一步失败即停止。恢复后的启动使用 `--open-world` 严格加载已发布的检查点。

```sh
cargo build --locked -p rustcraft-host --no-default-features --bin rustcraft-server &&
sh -eu -c '
  demo=$(mktemp -d ./target/checkpoint-demo.XXXXXX)
  server=./target/debug/rustcraft-server
  printf "演示目录 %s\n" "$demo"
  "$server" --world "$demo/world" --seed 42 --ticks 16
  "$server" --verify-world "$demo/world"
  "$server" --backup "$demo/world" "$demo/backup"
  "$server" --restore "$demo/backup" "$demo/restored"
  "$server" --verify-world "$demo/restored"
  "$server" --open-world "$demo/restored" --ticks 5
  "$server" --verify-world "$demo/restored"
'
```

备份保留发布代号、Manifest 和该代全部记录。命令先验证记录校验、Schema 和模组锁定信息，再发布目标目录的 `CURRENT`。目标应为源世界之外的新目录，其父目录须已存在。恢复后用独立目录启动世界。

<a name="section-13-6"></a>

### 13.6 P3 离线格式迁移

在仓库根目录对已有 `CURRENT` 的世界运行以下命令。选择源世界之外的新目标目录，其父目录须已存在。成功时输出 `event=checkpoint_migrated`，随后可用 `--verify-world` 核对代号、Tick 和规则版本。

```sh
cargo build --locked -p rustcraft-host --no-default-features --bin rustcraft-server &&
target/debug/rustcraft-server --migrate-world ./world ./world-migrated &&
target/debug/rustcraft-server --verify-world ./world-migrated
```

迁移仅处理 `CURRENT` 指向的一代。它把 Manifest 升级至 Schema 8，按现有预设和模式字段选择玩家 CBOR 的 Schema 2–4，并使用延迟任务 Schema 2。区块 Region 文件、代号、Tick、世界种子、随机流、待处理任务和已选模式均保留。原世界与历史代留在源目录。迁移在发布前重新加载目标检查点，校验失败会退出。验证后用 `--open-world ./world-migrated` 继续运行。

<a name="section-13-7"></a>

### 13.7 P3 候选清单、指定代校验与恢复

在仓库根目录构建独立服务端。对带有 `checkpoints/` 的源世界先列出候选，再用清单中的实际代号替换示例中的 `2`。目标目录须为源世界之外的新目录，其父目录须已存在。

```sh
cargo build --locked -p rustcraft-host --no-default-features --bin rustcraft-server
target/debug/rustcraft-server --list-checkpoints ./world
target/debug/rustcraft-server --verify-generation ./world 2 &&
target/debug/rustcraft-server --restore-generation ./world 2 ./world-recovered &&
target/debug/rustcraft-server --verify-world ./world-recovered
```

候选按代号升序列出，包含代号、Manifest 的 Tick 和 `current` 标记。`CURRENT` 缺失或无法解析时，标记为 `unknown`。编号目录也可能来自中断的发布流程。`manifest_ok` 表示清单可读、Schema 和规则锁定通过检查。清单无法读取或校验时，该代显示 `manifest_error`。

`--verify-generation WORLD GENERATION` 只读完整校验指定代的 Manifest、Region、玩家数据与模组锁定。它可在源 `CURRENT` 损坏或缺失时使用。校验通过输出 `event=checkpoint_verified`，失败返回非零退出码。`--verify-world WORLD` 校验 `CURRENT` 指向的检查点。

`--restore-generation` 直接读取指定的编号目录，完整校验区块、玩家数据与模组锁定后发布目标 `CURRENT`。成功时输出 `event=checkpoint_restored`，保留所选代的原始格式和代号。源目录保持原样。随后用 `--open-world ./world-recovered` 继续运行。
