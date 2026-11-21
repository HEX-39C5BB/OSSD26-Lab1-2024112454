# 性能与可观测性

[文档索引](README.md)

<a name="section-14"></a>

## 14. 性能、资源预算和可观测性

<a name="section-14-1"></a>

### 14.1 预算类型与过载

分别核算模拟 Tick、后台工作队列、Chunk/Section 缓存、磁盘写回、网络优先级/背压、客户端 CPU/GPU/显存以及未来多视图的开销。允许延迟、取消或降级非权威任务；**不得为了维持 TPS 静默丢失必须执行的权威事件或改变指定的随机/红石语义**。Tick 过载需输出诊断；允许的实际时间延迟与版本化规则保持一致。

<a name="section-14-2"></a>

### 14.2 调试与日志

按 Tick、Dimension、Chunk、Entity、System、Mod、Async Task 关联结构化日志、指标、CPU/GPU 性能分析、事件/随机状态记录和必要的快照 Hash。高开销追踪按需启用。避免在日志和故障报告中泄露认证令牌、账户凭据及敏感玩家数据。完整 Replay/Time Travel 后置。

<a name="section-14-3"></a>

### 14.3 性能验收原则

性能必须在固定硬件、可重复世界种子、渲染分辨率/视距、明确规则预设、模组集合与测试脚本上报告。至少分别评估 Tick P50/P95/P99、后台任务队列、Chunk 加载/保存、Mesh 构建、GPU 帧时间和内存/显存占用。性能达标不能代替逻辑兼容性验证。

P2 将区块视距设为可配置项。P3 第一轮以 [§15.4](development.md#section-15-4) 的 Gentoo 笔记本为参考设备，选择区块首次生成与驻留区块激活、停用作为无窗口基线。使用种子 42，分别采样半径 1、2、4，对应峰值活动区块数 9、25、81。后续图形采样同时记录实际选中的 GPU 与驱动版本（TBD-017）。

```sh
cargo build --release --offline --locked -p rustcraft-host --no-default-features --bin rustcraft-server
target/release/rustcraft-server --benchmark --radius 1
target/release/rustcraft-server --benchmark --radius 2
target/release/rustcraft-server --benchmark --radius 4
```

每次以独立进程运行。日志记录启动世界的 9 个区块及扩展区域的首次生成耗时。预热 64 个 Tick 后，默认采样 2048 个 Tick。每个样本测量一个权威 `Server::step`，包含一批区块状态变更和快照发布。中心区块持续活动，外围区块每 Tick 交替停用或重新激活。停用的区块仍驻留内存，进程峰值 RSS 取 Linux `VmHWM`。报告分别给出两种转换的 Tick P50、P95、P99，单位为纳秒。采样不经过 20 Hz 等待和 Host 消息队列。使用 `--ticks N` 可指定偶数样本量。

#### Gentoo 首轮实测

使用 Release 构建和 `chunk_lifecycle_v1`，内建模组为 `rustcraft@0.1.0` 与 `rustcraft_content@0.2.0`。每档运行一次，激活和停用各采样 1024 个 Tick。半径 1 的扩展耗时为 0，因为启动区域已覆盖该档。

| 半径 | 活动区块峰值 | 启动 / 扩展 / 总准备（ns） | 停用 P50 / P95 / P99（ns） | 激活 P50 / P95 / P99（ns） | 峰值 RSS（KiB） |
|---:|---:|---:|---:|---:|---:|
| 1 | 9 | 509353 / 0 / 528646 | 2044 / 2441 / 2498 | 1885 / 2273 / 2357 | 3516 |
| 2 | 25 | 614624 / 841983 / 1490438 | 5357 / 7171 / 7361 | 4619 / 6167 / 6241 | 3476 |
| 4 | 81 | 269305 / 1717741 / 2010502 | 7181 / 13752 / 22759 | 6040 / 11223 / 12964 | 3600 |

半径 4 的首次扩展生成 72 个区块，耗时约 1.718 ms。该档驻留切换的较高 P95 为停用阶段的 13.752 µs，约占 20 Hz 单 Tick 50 ms 周期的 0.028%。三档峰值 RSS 接近，单轮测量不足以确认内存随半径变化的趋势。首轮客户端基线使用 Intel Raptor Lake-S 核显，记录帧时间、区块网格耗时和实际驱动版本。负载阈值留待重复采样。

#### 客户端图形基线

首轮客户端基准使用种子 42、内置资源和固定的 1920×1080 像素离屏目标。镜头固定位置、高度和俯仰，按采样帧数匀速转一圈。分别运行半径 1、2、4 的独立进程，对应 9、25、81 个活动区块。

```sh
cargo build --release --offline --locked -p rustcraft-host --features client --bin rustcraft-client
target/release/rustcraft-client --terrain-benchmark 1
target/release/rustcraft-client --terrain-benchmark 2
target/release/rustcraft-client --terrain-benchmark 4
```

默认选择支持时间戳查询的 Intel 核显。以 `event=vulkan_terrain_device` 记录实际设备和驱动。每档记录冷网格构建耗时和四边形数，随后预热 30 帧并采样 360 帧。日志分别给出 CPU 帧准备、Vulkan 时间戳测得的地形与 HUD 绘制，以及从 CPU 准备到 GPU 栅栏完成的总耗时 P50、P95、P99，单位为纳秒。离屏采样使用单帧在途和重复使用的渲染资源。交换链首帧仍按 [§15.4](development.md#section-15-4) 验收。`--frames N` 可调整采样帧数，`--gpu auto` 可在其他 Vulkan 设备上检查基准流程。参见 [Vulkan 时间戳查询](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdWriteTimestamp2.html)。

#### Intel 核显首轮实测

Gentoo 参考机以 Release 构建运行 `flat_rotation_v1`，使用内置模组和资源。三档均实际选中 Intel(R) Graphics (RPL-S)，驱动为 Mesa 26.1.8，Vulkan 1.4.354。每档运行一次，分位数按 P50 / P95 / P99 排列。

| 半径 | 活动区块 | 网格四边形 | 冷网格（ns） | CPU 准备（ns） | GPU 渲染（ns） | 离屏总帧时间（ns） |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 9 | 152 | 6007747 | 62370 / 74471 / 85076 | 314479 / 444635 / 490677 | 875618 / 3205138 / 5336894 |
| 2 | 25 | 236 | 26775276 | 93267 / 107930 / 122634 | 391458 / 550833 / 608750 | 1031296 / 3219466 / 5169367 |
| 4 | 81 | 458 | 46918270 | 189167 / 227384 / 315450 | 411771 / 608490 / 650573 | 1468704 / 3938745 / 5093199 |

首版游戏默认视距定为半径 2（25 个活动区块），高视距档为半径 4（81 个活动区块）。本轮半径 2 的离屏总帧 P95 为 3.219 ms，冷网格耗时 26.775 ms。半径 4 的冷网格比半径 2 多约 20.143 ms。视距配置和玩家跨区块更新已通过自动化验证，真实窗口帧间隔见下文。

<a name="streaming-window"></a>

#### 真实窗口区块流送采样

从 Gentoo 桌面终端运行 Release 构建，保持窗口可见且尺寸不变。基准默认选择 Intel 核显，按实际 Vulkan 交换链大小核对 1920×1080 像素。`--gpu auto` 用于其他设备的流程检查，记录时以 `event=vulkan_terrain_device` 为准。

```sh
cargo build --release --offline --locked -p rustcraft-host --features client --bin rustcraft-client
target/release/rustcraft-client --streaming-benchmark 1
target/release/rustcraft-client --streaming-benchmark 2
target/release/rustcraft-client --streaming-benchmark 4
```

`flat_round_trip_v1` 使用种子 42、内置资源、20 Hz 本地权威模拟和原生 Vulkan FIFO 呈现。区块与网格就绪后预热 30 帧，玩家沿正 Z 方向跨越三个新区块，随后沿原路回到起点。去程扩展生成地形，回程重新激活驻留区块。基准使用内存世界，无存档和设置写入。

两段各自报告活动区块缺口与帧后待建网格数的峰值，同时以纳秒给出成功呈现提交之间的间隔和预算内网格更新 CPU 耗时的 P50、P95、P99 与最大值。每次跨界输出一条 `event=streaming_benchmark_crossing`，记录从首次呈现新区块视野到活动区块和网格全部补齐的时间、两端的权威 Tick，以及期间最大的呈现间隔和网格更新耗时。首帧的呈现间隔计入峰值，计时则从该帧呈现起算。`settled=false` 表示进入下一区块时视野仍未补齐。

跨界记录还包含 `generated_chunks` 和 `reactivated_chunks`。基准逐 Tick 对比权威快照。此场景从新世界开始，停用的区块保持驻留，因此区块首次进入活动集计为生成，离开活动集后再次进入计为重新激活。计数包含首次呈现跨界帧之前已经完成的区块激活。完整补齐的一次跨界会激活一整排区块，半径 1、2、4 分别对应 3、5、9 个。去程应为首次生成，回程应为重新激活。

检查 `event=streaming_benchmark_surface width=1920 height=1080`、两段各三条跨界记录和 `event=streaming_benchmark_complete`。复测时同时保留 `settled=false` 的记录。尺寸不符、路线异常、提前关闭或超时会返回失败。

#### Gentoo 首轮窗口实测

用户在 Gentoo 参考机上以 Release 构建运行 `flat_round_trip_v1`，每档一次。三档均确认 1920×1080，使用 Intel(R) Graphics (RPL-S) 核显、Mesa 26.1.8 和 Vulkan 1.4.354。以下时间为毫秒，按 P50 / P95 / P99 / 最大值排列。

| 半径 | 路段 | 帧数 | 活动区块缺口 / 待建网格峰值 | 呈现间隔（ms） | 网格更新 CPU（ms） |
|---:|---|---:|---:|---:|---:|
| 1 | 去程 | 640 | 1 / 3 | 16.142 / 16.264 / 16.404 / 21.343 | 0.010 / 0.011 / 3.631 / 5.355 |
| 1 | 回程 | 638 | 1 / 3 | 16.143 / 16.305 / 16.478 / 21.395 | 0.010 / 0.012 / 3.925 / 5.339 |
| 2 | 去程 | 639 | 3 / 5 | 16.148 / 16.283 / 20.214 / 21.496 | 0.027 / 0.031 / 4.900 / 5.460 |
| 2 | 回程 | 638 | 3 / 5 | 16.143 / 16.295 / 19.755 / 21.527 | 0.027 / 0.034 / 4.601 / 5.547 |
| 4 | 去程 | 639 | 7 / 9 | 16.145 / 16.318 / 21.138 / 21.488 | 0.076 / 0.114 / 5.218 / 5.497 |
| 4 | 回程 | 638 | 7 / 9 | 16.143 / 16.310 / 20.988 / 22.748 | 0.074 / 0.106 / 5.441 / 6.699 |

三档的呈现间隔 P95 均约 16.3 ms，受客户端 16 ms 重绘节拍影响。半径 2 的最慢帧去回程接近。半径 4 的最长呈现间隔与网格更新时间都出现在回程。去程包含新地形生成，回程复用驻留区块，但仍需要重建网格。这组路段汇总无法把长帧归因到单一阶段。逐次补齐时间与验收上限见下文。

#### 重复采样与性能门槛

基于 Gentoo 同一台 Intel 核显参考机的六份复测日志，每档独立运行三次。36 次跨界全部补齐，半径 2 每次激活 5 个区块，半径 4 每次激活 9 个。去程均首次生成，回程均重新激活。补齐跨过的权威 Tick 数与每 Tick 两个区块的激活预算一致。

| 半径 | 路段 | 跨界次数 | 补齐 Tick 差 | 补齐最小 / 中位 / 最大（ms） | 呈现间隔最大（ms） |
|---:|---|---:|---:|---:|---:|
| 2 | 去程 | 9 | 2 | 95.410 / 96.776 / 113.953 | 24.314 |
| 2 | 回程 | 9 | 2 | 95.653 / 97.357 / 113.078 | 21.497 |
| 4 | 去程 | 9 | 4 | 192.735 / 193.081 / 208.717 | 21.676 |
| 4 | 回程 | 9 | 4 | 192.368 / 193.142 / 209.559 | 21.905 |

首版性能门槛以每次跨界的视野补齐耗时 `elapsed_ns` 为准，半径 2 上限为 **135 ms**，半径 4 为 **235 ms**，呈现间隔继续记录。两档上限比本轮最长样本分别多 21.047 ms 和 25.441 ms。半径 1 保留采样入口，首版门槛覆盖半径 2 和 4。

在 Gentoo 参考机上复测后运行 [日志检查工具](../tools/check_streaming_baseline.py)。检查要求六份完整 Release 日志、每段三个跨界样本且全部补齐。超限、未补齐或日志缺失会返回非零状态。以下命令按半径和运行序号将日志保存在 `target/streaming-baseline.*`，随后执行检查。

```sh
cargo build --release --offline --locked -p rustcraft-host --features client --bin rustcraft-client &&
bash -e -o pipefail -c '
  log_dir=$(mktemp -d target/streaming-baseline.XXXXXX)
  printf "日志目录 %s\n" "$log_dir"
  for radius in "$@"; do
    for run in 1 2 3; do
      target/release/rustcraft-client --streaming-benchmark "$radius" 2>&1 |
        tee "$log_dir/radius-$radius-run-$run.log"
    done
  done
  python3 tools/check_streaming_baseline.py "$log_dir"
' bash 2 4
```

复核已保存的日志时可直接运行 `python3 tools/check_streaming_baseline.py target/streaming-baseline.6cTrUc`。该目录的 36 次跨界通过首版门槛。

#### 视距 1–16 的采样节点

P3 收口在 Gentoo KWin Wayland 上使用 Intel 核显，固定实际帧缓冲为 1920×1080。采样半径 1、4、8、12、16，分别对应 9、81、289、625、1089 个活动区块。五档各保存一份完整路线日志。半径 4 沿用 235 ms 补齐门槛，其余档位记录补齐结果供基线评估。半径 2 的历史门槛见上一节。

五档已通过本地无窗口区块生命周期基准和 SwiftShader 离屏绘制检查，活动区块数符合预期。Gentoo 实测见[五档结果](#gentoo-expanded-results)。

```sh
bash -e -o pipefail -c '
  test -n "${WAYLAND_DISPLAY:-}"
  gpu=$1
  cargo build --release --offline --locked -p rustcraft-host --features client --bin rustcraft-client
  log_dir=$(mktemp -d "target/streaming-expanded-$gpu.XXXXXX")
  printf "日志目录 %s\n" "$log_dir"
  for radius in 1 4 8 12 16; do
    env -u DISPLAY target/release/rustcraft-client --streaming-benchmark "$radius" --gpu "$gpu" 2>&1 |
      tee "$log_dir/radius-$radius.log"
  done
  python3 tools/check_streaming_baseline.py --expanded --gpu "$gpu" "$log_dir"
' _ intel
```

日志检查要求每档记录选定 GPU、驱动版本、实际 1920×1080 帧缓冲、来回各三次跨界及正常结束。Intel 的半径 4 须补齐并符合已有门槛。其余档位保留未补齐记录，用于评估扩展视距。

在同一 Wayland 会话补测 NVIDIA 独显时，将命令最后的 `intel` 改为 `nvidia`，使用相同路线和档位。客户端会检查独显是否支持向当前 Wayland 窗口呈现，日志检查验证实际 GPU 为 NVIDIA。补测结果用于建立独显基线。

<a name="gentoo-expanded-results"></a>

#### Gentoo 五档窗口实测

2026-11-19 在 Gentoo KWin Wayland 参考机运行 P3 本地工作区的 Release 构建。十次运行均确认实际 1920×1080 帧缓冲，日志未记录构建 SHA。Intel(R) Graphics (RPL-S) 使用 Mesa 26.1.8、Vulkan 1.4.354。NVIDIA GeForce RTX 4060 Laptop GPU 使用 NVIDIA 595.112.07、Vulkan 1.4.329。

| 半径 | 活动区块 | Intel 补齐最大（ms） | NVIDIA 补齐最大（ms） | Intel 呈现间隔最大（ms） | NVIDIA 呈现间隔最大（ms） |
|---:|---:|---:|---:|---:|---:|
| 1 | 9 | 47.003 | 47.781 | 22.784 | 22.305 |
| 4 | 81 | 207.956 | 209.386 | 25.252 | 24.523 |
| 8 | 289 | 401.231 | 406.262 | 24.402 | 26.188 |
| 12 | 625 | 610.417 | 612.581 | 25.192 | 25.352 |
| 16 | 1089 | 805.723 | 803.992 | 23.191 | 23.482 |

两块 GPU 在每档的 6 次跨界均补齐，共 60 次。Intel 半径 4 通过 235 ms 门槛。补齐计时跨过的权威 Tick 数在每档均等于半径，与每 Tick 两个区块的激活预算一致。扩展档位和 NVIDIA 结果作为基线保存。

在仓库根目录复核用户提供的原始日志。

```sh
python3 tools/check_streaming_baseline.py --expanded --gpu intel target/streaming-expanded-intel.gtaTZR
python3 tools/check_streaming_baseline.py --expanded --gpu nvidia target/streaming-expanded-nvidia.hmkRc0
```
