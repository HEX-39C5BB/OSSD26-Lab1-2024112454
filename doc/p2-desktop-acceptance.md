# P2 桌面键鼠验收

当前验收环境为 Gentoo Linux x86-64、KDE Plasma / KWin Wayland 和 Intel 核显。X11 暂不支持。请在本机已登录的 Wayland 桌面终端，从仓库根目录运行。NVIDIA 独显的流送补测见 [窗口基准](performance.md#streaming-window)。

用户已确认此前的桌面键鼠验收通过，并报告恢复时需要额外点击一次。本轮回归重点是自动恢复鼠标捕获和新增的方块反馈。

## 检查窗口首帧

```sh
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- --terrain-smoke
```

窗口呈现一帧后自动退出。确认退出码为 0，终端出现 `event=vulkan_terrain_first_frame_presented`，并记录 `event=vulkan_terrain_device` 中实际选中的 GPU 与驱动。

当前使用本地桌面命令验收首帧。远端 GPU 工作流延期至云服务器确定后，见 [TBD-024](decisions.md#section-19) 和 [P3 验收清单](p3-acceptance.md)。

## 启动独立验收世界

```sh
cargo run --locked -p rustcraft-host --features client --bin rustcraft-client -- \
  --world ./target/keyboard-acceptance-world \
  --settings ./target/keyboard-acceptance-controls.json
```

首次验收请选择尚未使用的世界目录和设置文件名。再次启动时将 `--world` 改成 `--open-world`，使用同一组路径。键位按物理位置识别。

| 操作 | 通过标准 |
|---|---|
| 首次左键点击窗口，水平和垂直移动鼠标 | 首次点击只捕获鼠标，指针隐藏，视角随鼠标移动。 |
| 分别按住和松开 W、A、S、D，按空格跳跃 | 前后和左右移动方向正确，松手即停止水平移动，跳跃后正常落地。 |
| 快速按下并松开空格两次，再松开所有键。按 `F3` 查看状态 | 玩家升空并停在空中，F3 显示 `FLYING`。单次按住空格只会普通跳跃。 |
| 悬停时再快速按两次空格 | F3 切换为 `WALK`，玩家按普通重力下落并在触地后继续行走。 |
| 飞行时按住空格升高，再用 WASD 平移，最后按住 Shift 下降到地面 | 上升、平移和下降均有碰撞。脚接触地面后 F3 显示 `WALK`，WASD 恢复地面移动。 |
| 按 `2`，瞄准身前约两到四格处的地面并等准星变黄，右键一次 | 准星瞄准的方块有黄色描边，第二槽高亮，放置橙色壤土，数量从 8 变为 7。瞄准刚放置的橙色块左键一次，方块消失，数量回到 8。两次成功都应使准星短暂变绿。 |
| 按 `4` 选空槽，保持对着可放置的地面，右键一次 | 准星短暂变红，场景和库存数量保持。可在终端看到 `EmptySelectedSlot`。 |
| 起跳后立即按 `Esc` | 指针恢复，菜单从 `PAUSING` 变为 `PAUSED`，世界停在当时的高度。等待两秒，移动鼠标并按 W、空格和数字键，视角、位置和选中槽保持。若按下时已落地，可重新起跳测试。 |
| 在暂停菜单用上下方向键选择 `SENSITIVITY`，按 `→` 一次。选 `INVERT Y`，按 Enter | 默认 100% 变为 125%，`INVERT Y` 变为 `ON`。选择 `RESUME` 后按 Enter 或按 `Esc` 继续。若暂停前已捕获且窗口仍有焦点，直接移动鼠标或按 W 即可操作。向上推鼠标时视角向下，水平观察方向保持。 |
| 用新设置文件启动，按 `Esc` 后按 `↑` 选择 `VIEW DISTANCE`，按两次 `→` 将默认值改为 4 | 每次调整一级，暂停期间世界保持冻结。按 `Esc` 恢复后，终端打印 `event=view_distance_applied radius=4`，近处地形优先补齐。正常关闭并使用同一设置文件重启，菜单仍显示 4。 |
| 将 `VIEW DISTANCE` 改为 1 并恢复，放置一个标记块，沿直线走约 40 格后返回 | 跨区块时视距随玩家移动，地形逐步补齐。返回后标记块仍在原处。 |
| 单独按下并松开 `F3`，再移动、转向并瞄准地面 | 调试 HUD 显示坐标、区块与区段、朝向、Tick、FPS 和帧间隔。视距和网格数量随加载变化，准星命中时显示目标坐标与版本。 |
| 将视距设为 4，按住 `F3` 再按 `G`。向上飞过一个 16 格区段，再越过一个水平区块边界 | 全高黄色角线限于以玩家为中心的 3×3 区块。绿色逐层横线和青蓝色区段线只在当前区块，竖直覆盖当前及上、下各 3 个子区块。范围随玩家跨界更新。再按组合键隐藏调试线，F3 HUD 保持原可见状态。 |
| 按住 W 时用 Alt+Tab 切走，在另一个窗口松开 W，再切回 | 指针已释放，玩家停止移动。左键重新捕获后，按 W 才重新向前。 |
| 放置一块方块，确认数量变化后，关闭窗口并重开 | 终端打印 `event=checkpoint_saved`。重开后刚放置的方块、库存数量和玩家位置恢复，菜单保留鼠标设置。 |

若菜单刚打开时出现 `PAUSING`，以变为 `PAUSED` 的时点作为冻结检查起点。若鼠标无法捕获，记录终端中的 `event=cursor_capture_failed`、当时使用的窗口系统和 GPU。若出现 `StaleBlockTarget`，先停住并瞄准，等准星稳定后再点击。

省略 `--settings` 时，客户端使用 `$XDG_CONFIG_HOME/rustcraft/controls.json`。未设置 XDG 路径时使用 `~/.config/rustcraft/controls.json`。

用户已在 Gentoo 桌面验收 F3 调试 HUD 和首版 F3+G 区块边界。新版的颜色、3×3 角线范围和横线高度按上表验收。跨区块加载对窗口帧间隔的影响按 [§14.3](performance.md#section-14-3) 的真实窗口基准采样。参见 [真实窗口区块流送采样](performance.md#streaming-window)。

视距扩展后的回归只验收半径 1、4、8、12、16。保持玩家位置不动并等待 F3 的待建网格归零，活动区块应分别达到 9、81、289、625、1089。半径 16 从最小视距扩展时，按每 Tick 激活两个区块的预算需要约 27 秒。

用户已确认上一轮起飞、下降触地恢复行走和视距测试通过。空中双击取消飞行按上表复验。

## 验收声音

Linux 客户端构建需要 ALSA、PipeWire 开发库和 libclang。可用 `pkg-config --modversion alsa libpipewire-0.3` 检查开发库。运行时优先打开原生 PipeWire，连接不上时由 CPAL 选择 ALSA。

在桌面终端运行音效检查。它只使用终端。先听两秒参考音，再听以 100% 游戏音量播放的放置、破坏和脚步声。查看 `event=audio_output_selected` 确认打开的设备。混音完成后输出 `event=audio_smoke_passed`，仍需确认实际听到了声音。流错误或播放超时会返回非零退出码。

```sh
cargo run --offline --locked -p rustcraft-host --features client --bin rustcraft-client -- --audio-smoke
```

在运行 PipeWire 的桌面，预期输出设备日志包含 `backend=PipeWire`。若显示 `backend=ALSA`，用 `wpctl status` 检查桌面终端能否连接到 PipeWire 会话。

Gentoo / KDE Plasma 实机已通过 `--audio-smoke`。原生 PipeWire 以双声道、48 kHz 播放，缓冲请求为 1024 帧。用户确认听到了参考音、放置声、破坏声和脚步声，日志输出 `event=audio_smoke_passed`。

按上面的「启动独立验收世界」打开游戏窗口，再检查下列行为。

| 操作 | 通过标准 |
|---|---|
| 走过几格平地，随后顶着墙走或跳起 | 着地移动时有脚步声。顶墙原地不动或腾空时脚步声停止。 |
| 放置和破坏一次成功的方块操作，再用空槽尝试放置 | 成功操作应同时出现准星绿闪和相应的声音。空槽操作只有红闪。 |
| 播放音效时按 Esc，等待暂停后继续 | 暂停时停止音效，继续后下一次成功操作或步行触发新音效。 |
| 在暂停菜单调低 `MASTER VOLUME`，再切换 `MUTE` | 音量作用于三种声音。静音时无音效，关闭静音恢复设定音量。重启后两项设置保留。 |

若测试通过但听不到声，记录参考音是否可闻，检查 Plasma 中 RustCraft 播放流的音量、静音和实际输出设备。若设备初始化失败，记录 `event=audio_unavailable` 或 `--audio-smoke` 的报错及使用的桌面音频系统。
