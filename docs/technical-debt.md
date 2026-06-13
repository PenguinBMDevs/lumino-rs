# 技术债务备忘

> 给屎山检查器 + 开发者：以下问题已知，尚未处理。

---

## Message 枚举暂不拆分

`crates/message/src/lib.rs` 中的 `Message<W, S, Se, T>` 枚举有 50+ 变体，是一个 God Message。

**为什么不拆**：`Message` 是 iced UI 框架的核心消息类型，被所有 handler 引用。拆分需要同时修改跨多个 crate 的路由逻辑，属于破坏性变更。建议在下次架构重构迭代中单独执行。

---

## 其他已知问题

- `crates/core/src/editor_state.rs` (762 行) — `EditorState` 职责过重，含所有编辑器状态 + 业务逻辑
- `crates/ui/src/editor/onion_bg_pool.rs` — `POOL_SIZE = 256` 硬编码 512MB GPU 纹理，无自适应
- `src/runner/inner.rs` — `RunnerInner` 5 层嵌套状态，链式访问深

---

## 已清理 (2026-06-13)

| 问题 | 操作 |
|------|------|
| `compute_selection` / `get_notes_in_selection_box` 重复逻辑 | 合并 |
| `save_storage` 重复字段比较 | 合并为 `config_diff` |
| `invalidate_onion_skin_cache_track` 死代码 | 移除 |
| `invalidate_onion_skin_colors` no-op 调用 7 处 | 清理 |
| `crates/event/` 零测试覆盖 | +7 测试 |
| `crates/message/` 零测试覆盖 | +36 测试 |
