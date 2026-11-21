# RustCraft 工程文档

从 [架构总览](architecture.md) 了解项目范围和模块边界。运行命令和游戏键位见[项目 README](../README.md)。维护工程文档时使用[文档标准](documentation-standard.md)。

## 架构规格

| 主题 | 原章节 | 内容 |
|---|---|---|
| [架构总览](architecture.md) | 0–3、22 | 产品范围、系统分层、工作区和术语 |
| [权威模拟、世界与交互](simulation.md) | 4–8、10 | Tick、规则预设、区块、生成、维度和玩家交互 |
| [Java 1.20.1 红石行为规格](java-redstone-model.md) | P4 专项 | GT、方向性供电、更新分发、QC/BUD 及实施顺序 |
| [Java 1.20.1 红石长期目标与达成情况](java-redstone-goal.md) | P4 专项 | 目标、P0–P2 达成状态、QC/BUD、验收差距与实施顺序 |
| [Java 红石参考测试设施审查](java-redstone-test-harness.md) | P4 专项 | 现有设施能力、缺口、最小增量计划与验收标准 |
| [客户端与图形](client.md) | 9 | 渲染、资源、键鼠、UI 和音频 |
| [模组、脚本与联网](extensions-and-networking.md) | 11–12 | 扩展契约、网络语义和服务器控制面 |
| [持久化、备份与恢复](persistence.md) | 13 | 存档格式、校验、严格打开和恢复命令 |
| [性能与可观测性](performance.md) | 14 | 区块流送、图形基准、参考机数据和性能门槛 |
| [开发路线与质量门禁](development.md) | 15–17、21、23 | 质量门禁、阶段路线、合规、追踪和实施状态 |
| [缺省约定与实施决策](decisions.md) | 18–19 | DFL、TBD 和阶段决策来源 |
| [RCF 架构决策登记](rcf-decisions.md) | 20 | RCF-001 至 RCF-089 的显式架构决策 |

## 验收与贡献

- [P1 实施决策](p1-decisions.md)和 [P1 验收](p1-acceptance.md)
- [P2 首个里程碑](p2-milestone-1.md)和[P2 桌面验收](p2-desktop-acceptance.md)
- [P3 验收清单](p3-acceptance.md)记录本地、远端 CI 和 Gentoo 实机证据。
- [P4 Java 1.20.1 兼容验证](p4-compatibility.md)记录参考环境、差分用例和验证范围。
- [Java 红石长期目标与达成情况](java-redstone-goal.md)汇总目标、完成状态和剩余差距。
- [Java 红石参考测试设施审查](java-redstone-test-harness.md)记录现有测试宿主能力、缺口和低侵入增量计划。
- [Java 红石参考实验](java-redstone-verification.md)和[红石兼容矩阵](java-redstone-coverage.md)记录逐 GT 观测、差分用例与证据状态。
- [分支命名规范](branch-naming.md)和[仓库贡献指南](../AGENTS.md)
