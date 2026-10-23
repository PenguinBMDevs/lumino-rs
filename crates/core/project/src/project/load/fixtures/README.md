# `.lmshapes` v1 回归夹具

- `shapes_v1.lmshapes`：由 **ShapeStyle v1→v2 改动前** 的代码（dev `c5b72d4b`）编码生成，111 字节。
- 内容：两个形状（Curve + Custom），覆盖 `anchors` / `rect` / v1 的 5 个样式字段 / `ops` / `k` / `name` / `hidden`。
- **禁止用当前代码重新生成**：新代码写出的字节永远「正确」，只有旧字节才能证明老工程仍能打开。
- 使用方：`data_formats/tests.rs`（v1 解码 + 升级默认值）、`save/tests.rs`（v1 → 保存 → v2 往返）。
