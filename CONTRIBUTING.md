# CONTRIBUTING

## 项目结构

项目使用私有 crates 进行模块化开发，位于 `crates/{module}`

- 对于该模块的目录，应当命名为其在项目中的地位，例：`core`
- 对于该模块的 crate 名称，应当附加上项目名称，例：`lumino-core`
- 命名需遵循 `kebab-case`，全部小写

## 代码规则

- 禁止使用 `unwrap()`，一切错误均需要被有效处理
- 不应使用 `{module}/mod.rs`，应使用 `{module}.rs` + `{module}/`

## 提交规范

- Commit Message 需遵循 [约定式提交 1.0](https://www.conventionalcommits.org/zh-hans/v1.0.0/) 标准
  - 需标注更改范围，例如 `feat(midi):`
  - 破坏性更新需使用 `[type]!`
- 所有提交均需使用 GPG 签名
- 提交前请运行 `cargo clippy` 检查代码质量，运行 `cargo fmt` 格式化代码

## 版本控制策略

- `master` 需始终保持已稳定、可发布的代码状态
- `dev` 日常开发分支
