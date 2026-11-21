# 客户端与图形

[文档索引](README.md)

<a name="section-9"></a>

## 9. 客户端图形、资源、UI、音频与特效

<a name="section-9-1"></a>

### 9.1 Render Hardware Interface

RHI 抽象 GPU 资源、命令缓冲、同步、交换链和设备特征。P2/MVP 图形后端使用 `ash`、`winit` 和 `ash-window`，以 Linux 上的 Vulkan 1.3 为设备基线，使用支持呈现的图形队列、交换链、动态渲染和 Synchronization2（TBD-006）。P1 的 Vulkan 1.0 探测记录属于既有实现，窗口首帧验收仍在追踪。

P2 按「世界绘制 → HUD → 呈现」组织有序渲染 Pass，并声明资源读写依赖。RHI 支持双帧在途，用帧栅栏管理复用，在相关 GPU 工作完成后释放资源。声明式 Render Graph、多视图与离屏渲染属于后续上层机制，P5 细化图与资源接口（TBD-007）。Apple 平台可研究 Vulkan 兼容路径，原生 Metal 后端按对应平台立项决定（TBD-016）。

<a name="section-9-2"></a>

### 9.2 Mesh / Culling / Submission

- Meshing、Visibility/Culling、GPU Draw Submission 三者分离。
- 普通完整方块使用面剔除与可合并面 Greedy Meshing；非完整方块、透明、液体几何采取独立策略。
- 生成任务使用区块快照、依赖版本和边界信息；已变更区块应使相关 Mesh 失效，过期完成结果不得覆盖新结果。
- GPU 裁剪、Indirect Draw、GPU 生成 Mesh、Mesh Shader 可作为后续可替换路径，不作为 MVP 前置条件。
- 模组注册模型、材质、有限 Shader/渲染阶段能力，但不允许直接无条件接管 Vulkan 设备。

<a name="section-9-3"></a>

### 9.3 游戏光照与视觉光照

权威模拟维护游戏规则所需的天空光/方块光查询和版本化传播策略；客户端维护用于着色的视觉光照。RGB 光照、动态光源、阴影、GI、光追与跨维度视觉光照可以逐步增强，但不得偷偷改变权威生物生成、作物或其他游戏逻辑判定。

<a name="section-9-4"></a>

### 9.4 统一资产系统

Asset Registry 持有规范化内部表示、命名空间、来源、内容哈希、缓存版本和资源依赖。Resource Provider/Importer 读取原创资产、用户已有合法安装、符合许可的官方来源及模组包，并转换到内部模型。转换失败和缺失资源需诊断及安全回退；**RustCraft 分发包不得未经授权捆绑 Minecraft 原版资产**。用户账号认证本身不自动赋予复制或再分发权。资源覆盖顺序受 Manifest 约束且可追踪。

P2 的原创资源使用目录包。`manifest.toml` 声明版本与依赖，贴图、结构化资源定义、音频分别采用 PNG、JSON、WAV。开发时可按同一源格式校验并转换，发布时由构建期打包器输出预处理资源包（TBD-013）。

首版地形采用高对比纯色。玄武岩为蓝色、壤土为橙色、地衣岩为青绿色，渲染叠加方块网格。原始 PNG 存于 `assets/native`。打包器生成 RGBA 材质数据以及可选 PCM 音频，并保存资源来源和 SHA-256。当前地形绘制使用材质代表色。

<a name="section-9-5"></a>

### 9.5 输入与 UI

平台输入抽象与 UI 交互状态、布局和绘制分离。游戏 HUD、快捷栏、物品栏及暂停和设置采用项目自有界面模型，调试面板使用 `egui`（TBD-014）。库存和机器 GUI 通过容器 Session 及权威交易操作更新 ItemStack。键鼠为 MVP，触摸、DPI、焦点、控制器和移动端布局逐步扩展。

本地客户端提供只读 F3 HUD，显示玩家坐标、区块与区段、朝向、权威 Tick、活动区块、视距、已建和待建网格，以及当前命中位置和区块版本。画面指标以成功呈现提交之间的间隔计算，显示最近帧间隔与平滑 FPS。单独 F3 在松开时切换 HUD，按住 F3 再按 G 独立切换区块边界调试线。

黄色角线覆盖以玩家所在区块为中心的 3×3 区块，共 16 根，沿世界高度绘制。横线沿当前区块四边绘制，竖直范围以玩家脚下的 16×16×16 子区块为中心，包含当前及上、下各 3 个子区块，并按世界高度裁剪。普通方块层使用绿色，16 格区段线沿用原青蓝色。调试线随玩家移动，参与世界深度和视锥裁剪。

<a name="section-9-6"></a>

### 9.6 音频、视觉效果与动画

- **Audio** 采用 `rodio` 接入客户端播放与混音。权威声音事件与客户端声音资源解析、空间策略、混音和平台后端分离，跨维度音频传播由独立能力定义（TBD-014）。
- **VFX：** 粒子是表现状态；爆炸伤害和火焰传播的权威结果独立。MVP CPU 粒子，后续 Vulkan GPU 粒子和多视图预算。
- **Animation：** 权威动作语义/时序与客户端控制器、姿态计算和绘制分离。MVP 玩家第一人称移动及简单部件动画，复杂骨骼/IK/重定向后置。
- **Weather & Sky：** 维度权威时间、天气与客户端天空/雾/云/降雨表现分离。

首批原创 WAV 覆盖放置、破坏与脚步。权威 Tick 在成功提交放置和破坏后发布声音事件，脚步以玩家着地时的实际移动距离为依据。`rodio 0.23.0` 将资源包中的 PCM 音效混合并居中输出。Linux 客户端通过 CPAL 优先连接原生 PipeWire，连接不上时使用 ALSA。默认输出设备优先请求 1024 帧，并按设备报告的范围调整。固定值被拒绝时采用设备默认值。暂停清除正在播放和排队的音效，继续后播放新事件。设备初始化失败时记录诊断并继续运行客户端。

<a name="section-9-7"></a>

### 9.7 P2 第一人称客户端与验证入口

```sh
python3 tools/pack_assets.py assets/native assets/native.rpak
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --world ./world
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --terrain-smoke
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --audio-smoke
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --preview target/terrain.ppm
```

打包器使用 Python 3.11 或更新版本与 Pillow。客户端内置预处理资源包，也可用 `--pack FILE` 载入另一个独立资源包。客户端指定 `--world` 时，在正常关闭后保存本地世界。

左键点击窗口捕获鼠标。使用 WASD 移动、空格跳跃、鼠标观察、左键破坏、右键放置，`1`–`8` 选择快捷栏槽。准星与快捷栏基于权威快照显示选中目标、堆叠数量和当前槽。

在 7 个权威 Tick（约 350 ms）内独立按下两次空格切换飞行，持续按住或键盘自动重复只计一次。飞行中用 WASD 平移、空格上升、任一 Shift 下降，松开升降键时悬停。双击取消后立即按普通重力下落。按 Shift 下降触地也会自动恢复行走，侧面或向上碰撞仍保持飞行。F3 显示 `FLYING` 或 `WALK`。检查点保存飞行状态，重新加载时从保存高度恢复，升降键需要重新按下。

单人客户端通过 `Esc` 释放鼠标并请求暂停。在下一个 Tick 边界提交已受理的输入，然后清除按住的移动键，冻结权威 Tick。菜单显示 `PAUSED` 时已冻结。通过 `Esc` 或选中 `RESUME` 后按 Enter 继续。暂停前已捕获且窗口仍有焦点时自动恢复捕获。失去窗口焦点时清除按键并释放鼠标。

权威快照中的选中方块用黄色描边显示。方块操作成功提交时准星短暂变绿，操作被拒绝或点击无可用目标时准星短暂变红。

暂停菜单提供鼠标灵敏度、Y 轴反转、主音量、静音和视距。鼠标灵敏度以 25% 调整，范围 25%–300%，默认 100%。主音量在 0%–100% 之间以 10% 调整，默认 70%。静音保留音量值。用左右方向键逐级调整 1–16 的视距半径，恢复后生效。客户端将设置保存在 XDG 配置目录，也可通过 `--settings FILE` 单独指定。旧设置缺少视距时使用默认半径 2。参见 [桌面键鼠和声音验收](p2-desktop-acceptance.md)。

区块网格依据中心及四邻区块的快照版本缓存，跨区块边界剔除内面并合并同材质共面方格。Vulkan 1.3 路径使用动态渲染与 Synchronization2，窗口为两帧分别分配命令、顶点和深度资源。离屏入口已在 SwiftShader 上实际绘制地形与 HUD。窗口验收以出现 `event=vulkan_terrain_first_frame_presented` 和退出码 0 为准。
