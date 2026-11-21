# P4 Java 1.20.1 兼容验证

[文档索引](README.md)

红石专项见[长期目标与达成情况](java-redstone-goal.md)、[行为规格](java-redstone-model.md)、[参考实验](java-redstone-verification.md)和[兼容矩阵](java-redstone-coverage.md)。

首个目标为 Java Edition 1.20.1。P4 从单项行为差分开始，按用例记录通过的能力。RustCraft 的实现经现有版本化规则接口接入。参见 [TBD-001](decisions.md#section-19)和[证据管理](development.md#section-15-2)。

## 参考环境

Prism Launcher 的 `1.20.1` 实例清单中，`net.minecraft` 组件版本为 `1.20.1`。共享库中的 `minecraft-1.20.1-client.jar` 已核对，SHA-256 为 `56b71336d2b4fdffd197f56595b0da93e32a946f78f382a299b8f4b92758bb0f`。行为采样时另外记录 Java 运行时、启用的数据包及用例设置。

本地参考资源从客户端包和资源索引 5 提取，存放在 Git 忽略的 `local-resources/minecraft-java/1.20.1/`。提交用例时只记录重放所需的输入、可观察结果与来源。参见[资源约定](development.md#section-17-3)。

## 首批差分用例

用户确认同时推进超平坦、生存交互和红石。以下三项使用 Java Edition 1.20.1 的 Classic Flat，种子 42，关闭结构。保留用例的输入、观察点、参考结果和 RustCraft 重放结果。

| 用例 | 场景与观察点 | 当前证据 |
|---|---|---|
| `java1201-flat-layers` | 在 `x,z ∈ {-16,-1,0,15,16}` 采样 `y=-64..=-60`，比较基岩、两层泥土、草方块和空气。 | Java 客户端内置预设与维度数据提供层序，RustCraft 在两个种子和检查点重开后均通过。 |
| `java1201-survival-inventory` | 给予 11 个泥土，右键提取 6 个，再向空格放 1 个。左键拿取或放下一叠。生存中键从已有物品选到对应的快捷栏槽。核对方块放置、按住挖掘和松开取消后的堆叠。 | RustCraft 权威交互、保存恢复和权限专项通过。原版生存操作验收暂缓。 |
| `java1201-redstone-line` | `y=-60,z=3`。`x=12` 放灯，`x=13` 放拉杆，`x=14,15` 放红石粉，`x=16` 放朝东的默认中继器，`x=17` 放朝东的活塞，`x=18` 放平滑石头。记录粉末电平、灯熄灭、中继器变更、活塞伸缩与平滑石头位置。 | 已从 1.20.1 的 `net.minecraft.data.Main --reports` 生成方块状态报告。P0 元件已有选定逐 Tick 差分；该合并线路的完整原版轨迹仍待采集。RustCraft 测试覆盖活动区块间的传播、延迟和待办任务恢复。同层红石粉从邻格生成点、线和交叉连接，使用原版图集中选出的纹理。 |

RustCraft 通过独立版本化预设接入这些能力。原版数据报告记录方块状态取值，逐 Tick 的顺序和持续时间由原版运行轨迹确认。首条测试线路使用平面红石与默认延迟。补充用例覆盖跨区块的单格爬坡、坡口遮挡、活塞清除有电或无电的红石粉，以及跨区块同时拆除活塞头和底座。

## 构建并运行

在仓库根目录运行。打包需要 Pillow 和 FFmpeg。打包器按清单复制选中的 PNG 与 OGG 到忽略的 `flat-pack-source/`，生成 PCM 供客户端播放。提取的原文件保留。`--preset java-1.20.1` 创建经典超平坦世界，从本地 `classic-flat.rpak` 读取材质、云、天空资源和测试音效。世界后续以存档记载的预设继续运行。

```sh
python3 -B tools/stage_java_flat_pack.py
cargo run --offline --locked -p rustcraft-host --features client,dev-tools --bin rustcraft-client -- \
  --world ./target/p4-java-flat --preset java-1.20.1
```

启动时可传 `--gamemode creative` 或 `--gamemode survival`。优先使用显式选项，其次使用上次保存的模式。新世界和缺少模式字段的旧存档以创造模式启动。客户端和独立服务端沿用相同的模式选择规则。

新建红石参考世界时，可把示例的 `--world` 改成新目录并改用 `--preset java-1.20.1-reference-v3`。V3 纳入原版活塞重开语义，已有 V2 存档继续沿用其保存规则；`java-1.20.1-reference` 入口仍选择 V2。此预设开放红石块、玻璃、普通活塞、粘性活塞、观察者和红石火把。右键中继器可在 1–4 RT 间循环切换。观察者的观察面沿视线方向，红点背面输出脉冲。用 `/give @s minecraft:redstone_torch` 取用火把，点击方块顶面或侧面放置。使用 F3+R 查看活塞事件，使用服务端 `--list-tasks` 查看各元件的待办计划刻和活塞动作。已有世界由已保存的预设决定行为。[原版参考实验](java-redstone-verification.md)

用 `/give @s minecraft:comparator` 取用比较器，右键在比较和减法模式之间切换。蛋糕可提供模拟输入，用来观察背面输入及输出红石粉的强度。参照原版装置测试隔导体的读取与侧向锁定。[比较器实验](java-redstone-verification.md#comparator-evidence)

用 `/give @s minecraft:lever` 取用拉杆。在参考预设中可将它装在方块上方、侧面或底部。右键翻转后，直接相邻的灯和经支撑充能的灯可以同时点亮。移除支撑，拉杆脱落，两盏灯在 4 GT 后熄灭。[拉杆实验](java-redstone-verification.md#lever-evidence)

用 `/give @s minecraft:redstone` 取用红石粉。在参考预设中，新放的孤立粉末为十字，右键在点和十字间切换。把粉末分别放在相差一格的支撑方块上，可观察低处向高处的竖直纹理。用平滑石头或玻璃遮住低处粉末的上方，可比较两种材料对连接和信号的影响。[红石粉实验](java-redstone-verification.md#wire-shape-evidence)

可用 `/give @s minecraft:stone_button` 取用石按钮，也可从创造物品栏选择橡木按钮、石和橡木压力板以及两种测重压力板。右键按下按钮，玩家踩上压力板时发出信号。可把红石粉或灯接到侧面验证输出，再观察离开后对应的重新检测时刻。[按钮和压力板实验](java-redstone-verification.md#controls-evidence)

物品图标由同一打包命令生成。打包清单覆盖 Java 1.20.1 注册表的 1,255 个物品，当前 Java 预设可获取的物品仍为 24 个。红石粉、中继器、比较器、拉杆、火把和蛋糕沿用原版物品层贴图；方块、按钮及压力板按继承模型、面 UV、GUI 变换和草色生成透明图标。方块图标采用本地 GUI 定向光照，尚未做原版逐像素差分。图标统一用于目录、背包、快捷栏和光标堆叠。原始提取目录保持不变，模型依赖、图标及来源 SHA-256 清单保存在忽略的 `flat-pack-source/` 中。

注册表清单保存在 `tools/java_items_1_20_1.txt`。63 个条目使用回退：`air` 没有可视纹理；箱子、潜影盒、导管和饰纹罐使用代表纹理的立方体；旗帜、床、头颅和盾牌使用提取贴图的简化形状。图标为 64×64，图集上限提高到 8192，本地 pack 约 21 MB，仍只写入 Git 忽略的 `flat-pack-source/` 和 `classic-flat.rpak`。HUD 图标此前把 UV 覆盖为非负值，顶点着色器因此按地形投影，图标不会出现在物品栏；现在保留负 x UV，由屏幕空间路径绘制。

从仓库根目录运行 `python3 -B tools/test_java_item_icons.py` 验证合成模型的继承、透明层、回退和源文件保留，不需要原版资源。完成本地打包后，运行以下测试验证三个 Java 预设的全部可选物品、HUD 图标映射及离屏 GPU 绘制。

```sh
cargo test --offline --locked -p rustcraft-host --features client,dev-tools \
  --bin rustcraft-client every_java_catalog_item -- --ignored
cargo test --offline --locked -p rustcraft-host --features client,dev-tools \
  --bin rustcraft-client renders_hud_item_icons -- --ignored
```

本轮在创造模式下验收。按 `E` 打开物品栏。上方目录左键取 1 个，中键取 64 个。背包和快捷栏可左键整叠操作、右键逐个放入。右下角红色 `X` 删除光标持有的堆叠。世界中左键即时破坏，右键放置或操作拉杆，中键把当前目标方块选入快捷栏。放置和破坏均保持物品数量。游戏内滚轮向上选择前一格、向下选择后一格，首尾循环；数字键 `1`–`9` 可直接选择。触控板纵向每累计 40 像素切换一格，横向滚动忽略。快速滚动合并目标，并等待权威库存确认后继续提交。菜单、背包和命令行打开时滚动不切换快捷栏，F3+R 的当前刻自由镜头中可切换。

按 `/` 在左下角打开命令行，输入按实际大小写显示。`Tab` 补全当前词或展开公共前缀，再按可轮选，`Shift+Tab` 反向轮选，`Enter` 接受高亮候选。继续按 `Enter` 执行命令。左右键移动光标，行尾按右方向键可接受行内建议。候选以外的上下键浏览历史。`give` 与 `@s` 之间允许多个空格。游戏命令仅由受信任的本地操作员提交。

```text
/tp @s 12.5 -60 5.5
/tp @s ~16 ~ ~
/minecraft:teleport @s ^ ^ ^4 90 0
/give @s minecraft:smooth_stone 16
/give @s minecraft:repeater 4
/gamemode creative
/tick query
/tick rate 10
/tick freeze
/tick step 1s
/tick step stop
/tick unfreeze
/tick sprint 100t
/tick sprint stop
/freecam on
/freecam off
/redstone on
/redstone step
/redstone previous
/redstone next
/redstone off
```

本地 `tp` 与 `teleport` 支持绝对、相对和局部坐标，以及可选的偏航和俯仰。`give` 使用本预设注册的物品，`gamemode` 提供创造和生存。单人世界中 `@s`、`@p`、`@a` 和 `@r` 均指向本地玩家。实体选择器条件、任意物品 NBT、旁观和冒险模式待兼容。

`/tick` 沿用 Java 1.20.3 加入的命令语法，是 RustCraft 在 1.20.1 预设下也可使用的调试能力。世界默认 20 GT/s，`rate` 设置目标游戏刻速率。`freeze` 停止 GT 和延迟任务，玩家仍可移动。`step` 缺省推进 1 GT，时间支持 `t`、`s`、`d`，与 `sprint` 使用同一套单位。`query` 显示目标 GT/s、当前状态和上次 GT 耗时。菜单及 F3+R 仍有独立的本地暂停。[Java 1.20.3 发布说明](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-20-3)

## 逐 Tick 调试

独立服务端可对存档中的活塞、发射器和投掷器执行只读 QC／BUD 诊断和重放。[运行命令与字段](java-redstone-verification.md#bud-inspection)

`F3` 在左上角显示玩家、世界及摄像机信息，在右上角显示 CPU、实际 Vulkan 设备、驱动、交换链格式和渲染管线。玩家与摄像机速度分别显示，单位为方块每秒。

默认 Debug 构建或启用 `dev-tools` 的构建支持 `F3+R`。进入时在权威 GT 边界暂停，并自动开启自由摄像机。`WASD` 移动镜头，空格和 Shift 升降，鼠标调整朝向。瞄准拉杆右键将交互排到下一步，按 `Enter` 或使用 `/redstone step` 推进 1 RT（2 GT）。交互只在第一个 GT 执行。两刻各有独立快照，从奇数 GT 开始也推进完整的两刻。最新状态支持右键放置、创造模式左键破坏和中键选取。左右键按 GT 只读查看最近 256 刻，上下键翻阅事件。`F3+R` 或 `Esc` 退出并恢复入场前的摄像机状态，取消未提交交互。薄荷绿标示计划及提交成功的输入，红色标示拒绝。紫色框标示方块变更，橙色箭头标示传播，蓝色箭头标示信号消失，文字图例使用相同颜色。普通游戏可用 `/freecam on|off` 独立启停，镜头输入不提交玩家位移。

## 客户端回归验收

- 在不同区块内放置并拨动拉杆，分别朝四个方向观察。局部网格更新期间，旧网格应继续显示到替换完成。
- 在单格台阶上下铺红石粉，验证台阶侧有竖向纹理。用拉杆分别从高端和低端供电，阻挡坡口后连接和信号应一起断开。
- 从正面和侧面观察活塞伸缩。检查伸出的头板、连接杆和底座凹面。向活塞前方放有电或无电的红石粉，通电后粉末应被清除。破坏伸出的活塞头时应同时移除底座。
- 执行 `/tick freeze` 后退出命令行，确认玩家仍可移动。用 `/tick step 2t` 观察线路推进两刻，随后恢复冻结。执行 `/tick unfreeze` 恢复连续运行。

## 待修复的表现问题

- 方块放置和破坏目前各复用一种音效。
- 天空和太阳渲染存在问题，等待单独修复。

在仓库根目录运行专项自动验收。原版交互与红石逐 Tick 对照使用 Prism 的 1.20.1 测试世界。运行前记录 Java 运行时、数据包与初始状态。保留操作输入及对应坐标的方块、库存观察结果。

```sh
cargo test --offline --locked -p rustcraft-authority --test java_flat
cargo test --offline --locked -p rustcraft-host --lib
cargo test --offline --locked --workspace --all-features
```

## 物品图标与滚轮复验

本地打包及自动化已通过，桌面显示待实机确认。打开创造目录，检查草方块的顶面与侧面、普通/粘性活塞的顶面、薄压力板，以及红石粉、中继器和蛋糕的独立图标。分别取入快捷栏、背包及光标，确认图标一致且数量可读。依次向上和向下滚动，检查首尾循环，再快速连续滚动并交替使用数字键。打开背包、菜单及命令行滚动，确认原快捷栏选择保持。关闭窗口后重开存档，核对已确认的选择槽位。
