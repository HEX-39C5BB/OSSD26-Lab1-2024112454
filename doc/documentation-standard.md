# 文档标准

## 目录与命名

工程文档统一放在 `doc/`，从[文档索引](README.md)进入。仓库根目录保留 [README](../README.md) 和 [AGENTS](../AGENTS.md) 作为使用与贡献入口。

- 文件名使用小写英文、数字和连字符，扩展名为 `.md`。例如 `p2-desktop-acceptance.md`。目录索引使用 `README.md`。
- 以领域组织架构规格，以里程碑保存验收记录。新文档在索引中登记主题、适用范围及入口。
- 每份文档使用一个一级标题。构建和验收命令注明运行目录、前置条件和通过标准。

## 编号与引用

- 保留已发布的章节号、RCF、DFL、TBD 编号及 `EXPLICIT`、`DEFAULT`、`TBD` 标记。实施状态和决策来源分别记录。
- 仓库内链接使用相对路径。章节引用使用稳定锚点。把 `<a name="section-13-5"></a>` 放在标题前，再链接到 [§13.5](persistence.md#section-13-5)。已有锚点随章节一起迁移。参见 [GitHub 自定义锚点](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#custom-anchors)。
- 规则或数据格式变更时，同步更新对应领域文档、决策登记与验收记录。记录影响范围、兼容性、迁移和验证结果。

## 本地检查

在仓库根目录运行以下命令。脚本检查文档目录、文件命名、本地链接和 Markdown 锚点。失败时输出位置并返回非零退出码。检查跳过外部链接和围栏代码示例。

```sh
python3 tools/check_docs.py
```
