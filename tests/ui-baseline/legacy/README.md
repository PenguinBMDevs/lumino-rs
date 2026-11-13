# `iced/` —— 已退役的 iced 视觉基线（只读归档）

这 18 张 PNG（6 个场景 × 3 个 DPI）是 iced 时代 `capture_iced` 生成的原图。

**状态：已退役（2026-11-11）。**

- 这些文件**原样保留**，作为历史对照，不删除、不修改、不重新生成。
- `capture_iced` 二进制已删除，不再参与任何验收流程。
- 唯一视觉门禁改为 `capture_egui`（`tests/ui-baseline/egui/`，12 张，6 场景 × 3 DPI）。

## 为什么退役

最终交付标准是「代码中没有任何 iced 残留」。`capture_iced` 渲染的是**纯 iced 视图树**，
它要求 iced 侧的工具栏 / 状态栏 / 左栏 / 标题栏菜单 / 右栏图标列持续绘制像素——
这与移除 iced 直接冲突，且在纪律上又不允许重生成基线，于是形成死锁。

退役只改变**门禁方式**，不改变回归判据：任何视觉回归仍会被 `capture_egui` 的
逐像素比对拦下；上述 18 张原图保留下来供人工对照历史外观。

## 如何人工对照

```bash
# 生成当前 egui 截图到临时目录（不覆盖归档）
cargo run -q -p lumino-ui-egui --bin capture_egui -- --out-dir target/egui-now
```

如需与 iced 历史外观比对，直接打开本目录中的同名文件并排查看。
