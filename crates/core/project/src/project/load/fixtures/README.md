# `.lmshapes` 回归夹具

## v1

- `shapes_v1.lmshapes`：由 **ShapeStyle v1→v2 改动前** 的代码（dev `c5b72d4b`）编码生成，111 字节。
- 内容：两个形状（Curve + Custom），覆盖 `anchors` / `rect` / v1 的 5 个样式字段 / `ops` / `k` / `name` / `hidden`。
- **禁止用当前代码重新生成**：新代码写出的字节永远「正确」，只有旧字节才能证明老工程仍能打开。
- 使用方：`data_formats/tests.rs`（v1 解码 + 升级默认值）、`save/tests.rs`（v1 → 保存 → v2 往返）。

## v2

- `shapes_v2.lmshapes`：由 **ShapeAnchor `bend`（v2→v3）改动前** 的代码（dev `08f3b84f`）编码生成，145 字节。
- 内容：Curve（自定义入/出控制柄 + v2 样式含 `areas` / `borders`）+ Live（`ShapeOp::Live` 内嵌 `Stroke.anchors` 与 `from: Vec<ShapeModel>`）。
- 用途：v3 解码必须覆盖 **顶层 geometry 锚点** 与 **Live 内嵌锚点**；升级后 `bend` 默认 `Smooth`。
- **禁止用当前代码重新生成**：这是旧 v2 字节的唯一证据。
- 使用方：`data_formats/tests.rs`（v2 夹具解码 + v2→v3 默认值）。
