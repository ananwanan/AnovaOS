<!-- Translate first-party development language, purpose headers and Rust formatting requirements. -->
# 开发语言与 Rust 格式

[English](../../en/guides/CODING_STYLE.md) | **简体中文**

本规范适用于 `dev` 上的自有代码和贡献；内核、ABI、安全及验收要求见
[AGENTS.md](../../../AGENTS.md)。

## 语言

以下内容使用英文：

- 标识符、模块名、新建源文件名。
- 源码注释、Rustdoc（`///`、`//!`）、示例和 API 文档。
- `SAFETY:`、`TODO:`、`FIXME:` 及不安全 API 的 `# Safety` 说明；解释实际不变量/原因。
- 日志、诊断、错误、panic/assertion 消息和 CLI 帮助。
- 测试名、测试说明与验收标记。
- 提交消息、PR 标题/描述及新开发文档的英文主版本。

本地化资源、翻译、外部引文和专门测试其他语言的样本可保留相应语言。
第三方源码、版权和许可文本保持原样。

[README.md](../../../README.md) 是英文默认入口，[README.zh-CN.md](../../../README.zh-CN.md)
是中文版；共享事实、链接、命令和状态变化时在同一批改动中同步更新。
`docs/en/` 与 `docs/zh-CN/` 保持完整对应；翻译不改变源码、注释、Rustdoc 和开发消息的英文要求。

## 文件用途（强制）

每个新增/修改的自有文件在开头附近用英文说明职责和边界。Rust 用 `//!` 模块说明，
C/汇编/链接/配置/工作流使用对应注释语法；Markdown 在标题附近说明范围。
职责变化时更新用途。仓库清单与生成/不可注释文件例外见 [FILE_PURPOSES](FILE_PURPOSES.md)。
不要给上游声明或生成 lockfile 手工插入头部。

执行 `python3 scripts/check_docs.py` 与 `cargo xtask check-files`，
[DEV-001](REQUIREMENTS.md) 将其列为交付要求。

## Rust 格式

根目录 [rustfmt.toml](../../../rustfmt.toml) 是唯一格式依据，作用类似 C/C++ 的 `.clang-format`。

| 设置 | 项目约定 |
| --- | --- |
| 解析 edition | Rust 2024；新建自有 Cargo package 同样声明 `edition = "2024"` |
| 风格 edition | 显式固定 2024，保持编辑器和 CLI 一致 |
| 缩进 | 4 空格，不使用硬制表符 |
| 建议行宽 | 100 列，注释/字面量按 rustfmt 规则处理 |
| 换行 | LF，`newline_style = "Unix"` |
| import/模块 | 允许 rustfmt 在支持范围内排序 |
| 简写 | 优先字段初始化简写、`?`，不用 `try!` |
| 其他排版 | rustfmt 默认及默认小项启发式 |

仅启用稳定配置。没有明确理由不要添加 nightly 选项、嵌套配置或大范围 `#[rustfmt::skip]`。
不要仅为风格重排 vendor/生成代码；将它们排除于自有格式目标并保留上游布局。

M0 固定已测 Rust 工具链并包含 `rustfmt`、`clippy`，记录确切版本；后续本地/CI 使用该工具链，
不要为排版选择浮动 nightly。rustfmt 不检查英文或安全解释，这些仍需审查。

### workspace 格式

在仓库根目录使用固定工具链：

```sh
cargo fmt --all
cargo fmt --all -- --check
```

workspace 和固定工具链现已具备，以上命令可直接执行。
Clippy、构建与宿主/guest 测试按目标分别选择，格式通过不能证明内核行为正确。

### 编辑器

可启用 rust-analyzer 的 rustfmt 集成和保存时格式化。让编辑器使用项目工具链并发现根配置，
避免与其冲突的编辑器私有排版覆盖。

## 官方依据

- [Rustfmt 配置](https://rust-lang.github.io/rustfmt/)
- [Rustfmt 用法](https://github.com/rust-lang/rustfmt#configuring-rustfmt)
- [Rust 2024 风格 edition](https://doc.rust-lang.org/edition-guide/rust-2024/rustfmt-style-edition.html)
