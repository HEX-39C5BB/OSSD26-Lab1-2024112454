# 分支命名规范

新建分支使用 `<type>/<scope-or-issue>-<description>`。按主要变更选择 `type`，用模块、子系统或 `issue-<编号>` 标识范围，再用简短的英文描述说明变更。名称使用小写字母和数字，单词用连字符连接。关联 RCF 决策时，可用 `rcf-<编号>` 标识范围。

| 前缀 | 用途 | 示例 |
|---|---|---|
| `feat/` | 新功能开发 | `feat/chunk-storage` |
| `fix/` | Bug 修复 | `fix/entity-serialization` |
| `docs/` | 架构与开发文档 | `docs/rcf-071-transactions` |
| `refactor/` | 不改变行为的重构 | `refactor/ecs-scheduler` |
| `perf/` | 性能优化 | `perf/greedy-meshing` |
| `test/` | 添加和改善测试 | `test/world-persistence` |
| `ci/` | CI/CD 配置 | `ci/gpu-runner` |
| `build/` | 构建系统及依赖 | `build/vulkan-sdk` |
| `chore/` | 常规维护 | `chore/dependency-update` |
| `hotfix/` | 稳定版紧急修复 | `hotfix/save-corruption` |

例如，修复 Issue #42 的存档加载问题可使用 `fix/issue-42-save-loading`。创建前从目标基线分支切出新分支。

```sh
git switch -c fix/issue-42-save-loading
```
