# 🔍 lumino-rs 代码质量审查报告

> 审查日期: 2026-04-03  
> 修复日期: 2026-04-03  
> 审查范围: 全仓库代码质量扫描（贡献者规范合规 + 代码卫生 + 结构问题）  
> 审查员: Code Reviewer Agent  
> **状态: ✅ 全部修复完成**

---

## 📊 总览

```
┌─────────────────┬──────────┬─────────────────────────────────────┐
│ 🟠 严重度       │ 数量     │ 说明                                │
├─────────────────┼──────────┼─────────────────────────────────────┤
│ 🔴 阻塞（必须修）│ 10      │ 违反贡献者文档硬性规定               │
├─────────────────┼──────────┼─────────────────────────────────────┤
│ 🟡 建议（应该修）│ 8       │ 代码质量/可维护性问题                │
├─────────────────┼──────────┼─────────────────────────────────────┤
│ 💭 挑剔（最好有）│ 5       │ 小改进/风格统一                     │
└─────────────────┴──────────┴─────────────────────────────────────┘
```

---

## 🔴 阻塞项（违反 CONTRIBUTING.md 硬性规定）

### 1. 大量 mod.rs 违规（8 处）

**规则**: `CONTRIBUTING.md` 明确规定 `不应使用 {module}/mod.rs，应使用 {module}.rs + {module}/`

| # | 文件 | 行数 | 严重度 | 备注 |
|---|------|------|--------|------|
| 1 | `crates/ui/src/settings/mod.rs` | **335** | 🔴🔴 | 最大违规，包含完整业务逻辑（SettingsPanel + view + 样式） |
| 2 | `crates/collaboration/src/handlers/mod.rs` | 55 | 🔴 | 模块入口文件 |
| 3 | `crates/ui/src/view/mod.rs` | 63 | 🔴 | 模块入口文件 |
| 4 | `crates/ui/src/state/mod.rs` | 20 | 🔴 | 模块入口文件 |
| 5 | `crates/ui/src/root/editor_ops/mod.rs` | 398 | 🔴🔴 | 包含业务逻辑 |
| 6 | `crates/ui/src/settings/components/mod.rs` | 107 | 🔴 | 模块入口文件 |
| 7 | `crates/ui/src/settings/pages/mod.rs` | 311 | 🔴🔴 | 包含业务逻辑 |
| 8 | `src/services/mod.rs` | 53 | 🔴 | 模块入口文件 |

**修复方案**: 将每个 `mod.rs` 拆为 `{parent}.rs`（模块声明）+ `{module}/`（子目录）。对于包含业务逻辑的 mod.rs（如 `settings/mod.rs` 335 行），将逻辑移入子模块。

### 2. gfx crate note_renderer 重复结构

**文件**: `crates/gfx/src/note_renderer.rs` + `crates/gfx/src/note_renderer/types.rs`

`note_renderer.rs` 本身已声明 `pub mod types;`，但文件本身仍是 mod.rs 模式。应拆为 `note_renderer.rs`（仅声明 + re-export）+ `note_renderer/types.rs`。目前结构虽然功能正确，但违反了规范的命名约定。

### 3. gfx/Cargo.toml 缺少 workspace 继承

```toml
# 当前（hardcode）
version = "0.1.0"
edition = "2024"

# 应为
version.workspace = true
rust-version.workspace = true
homepage.workspace = true
license.workspace = true
edition.workspace = true
```

### 4. workspace 中重复依赖声明

`Cargo.toml` 中 `tokio` 被声明了两次（第31行和第63行），features 相同。应合并为一次声明。

### 5. 子 crate 未使用 workspace 依赖

以下 crate 中有独立版本声明，而非 `{ workspace = true }`：

| Crate | 未继承的依赖 |
|-------|-------------|
| `crates/export` | `tokio`, `zstd`, `bincode`, `encoding_rs` |
| `crates/dms` | `bytes`, `encoding_rs`, `num-bigint` |
| `crates/core` | `rayon`, `bincode`, `zstd`, `encoding_rs`, `ouroboros`, `memmap2` |
| `crates/collaboration` | `futures`（workspace 中已有）, `tracing-subscriber` |
| `crates/ui` | `tracing`, `serde_json`, `rfd`（且版本 0.14 vs workspace 0.15 不一致！）, `image`, `once_cell` |

**特别严重**: `crates/ui/Cargo.toml` 中 `rfd = "0.14"` 但 workspace 依赖中是 `rfd = "0.15"`，版本不一致可能导致编译冲突。

---

## 🟡 建议项（代码质量）

### 6. 超长文件（300+ 行）

| 文件 | 行数 | 建议 |
|------|------|------|
| `tests/collaboration_ui_test.rs` | 714 | 测试文件可接受，但考虑拆分为多个测试模块 |
| `crates/collaboration/src/client.rs` | 454 | 考虑将 WebSocket 连接逻辑、消息处理、状态管理拆分 |
| `crates/ui/src/editor/scrollbar_widget.rs` | 425 | 自定义 widget 文件偏长，但结构合理 |
| `crates/core/src/midi/loader.rs` | 419 | 已知有 mmap unsafe，建议封装 unsafe 到独立模块 |
| `crates/export/src/converter.rs` | 417 | 考虑按导出格式拆分 |
| `crates/gfx/src/note_renderer.rs` | 406 | 拆分渲染管线、uniform、vertex 逻辑 |
| `src/runner/inner.rs` | 394 | **高优先** - 包含 74 条注释，可能是遗留代码聚集地 |
| `crates/core/src/midi/event.rs` | 388 | 已知有 mmap unsafe |
| `crates/ui/src/editor.rs` | 388 | 拆分 view/update 分离 |

### 7. 废弃代码未清理

| 文件 | 行号 | 内容 |
|------|------|------|
| `crates/collaboration/src/client/connection.rs` | 全文 | 整个文件已标记废弃，但仍在 `client.rs` 中被 `pub mod connection` 引用 |
| `crates/ui/src/toolbar.rs` | 85, 91 | "已废弃，保留兼容性" 代码 |
| `crates/ui/src/toolbar/event.rs` | 27-29 | 废弃的事件变体仍在定义 |
| `crates/ui/src/message.rs` | 79-81 | 废弃的消息变体 |
| `crates/dms/src/node/types.rs` | 24 | `#[deprecated]` 标记的类型 |

**建议**: 废弃代码应在下一个大版本中移除，不要在代码库中无限期保留"兼容性"代码。

### 8. src/platform/macos.rs 质量问题

- **7 处 `.expect()`**: 虽然 CONTRIBUTING.md 仅禁止 `unwrap()`，但 `expect()` 在平台初始化代码中使用意味着如果菜单构建失败，程序直接 panic
- **残留 `todo!()`**: 第74行被注释掉的 `todo!()` 应该删除
- **大量注释掉的代码**: `_submenus`, `menu.map.get(id)`, `lumino_core::event::emit()` 等应清理
- **线程局部 OnceLock**: 当前实现可用，但整体模块明显是半成品状态

### 9. TODO/FIXME 散落

| 文件 | 行号 | 内容 |
|------|------|------|
| `crates/ui/src/editor/state.rs` | 17 | `TODO: 之后我们需要支持不同的调式/微分音` |
| `crates/export/src/dms.rs` | 225 | `TODO: create_data_node 暂时保留` |
| `crates/dms/src/reader/types.rs` | 107 | `TODO: Implement full parsing using lumino_dms` |

**建议**: 统一使用 issue tracker 管理 TODO，不要散落在代码注释中。

### 10. `pub mod connection` 引用废弃模块

`crates/collaboration/src/client.rs:25` 中 `pub mod connection;` 引用了已废弃的 `connection.rs`（整个文件只有注释，没有有效代码）。应移除。

---

## 💭 挑剔项

### 11. `#[allow(dead_code)]` 使用

| 文件 | 行号 |
|------|------|
| `src/services/collaboration_service.rs` | 259, 265 |
| `crates/export/src/dms.rs` | 227 |
| `crates/core/src/midi/managed_midi/loader.rs` | 145 (`#[allow(clippy::type_complexity)]`) |

`#[allow(dead_code)]` 应该是临时措施。如果代码确实不再需要，应该删除；如果是为后续功能预留的公共 API，考虑使用 `#[cfg(feature = "...")]` 控制。

### 12. Cargo.toml 中依赖声明风格不一致

- `crates/core/Cargo.toml` 中 `tokio` 在 `[dependencies]` 和 `[dev-dependencies]` 中重复声明，features 不同。这是合理的模式，但其他 crate 应统一采用。
- workspace 的 `[dev-dependencies]` 中 `iced_core`, `iced` 重复声明了 `[dependencies]` 中已有的依赖。

### 13. `crates/ui/src/settings/mod.rs` 的样式函数过多

该文件包含 6 个独立的 `create_*_style()` 函数，每个都返回 `impl Fn(&Theme) -> container::Style`。这些应统一迁移到 `settings/components/` 子模块中。

### 14. `src/runner/inner.rs` 注释密度过高

394 行文件中有 74 条注释（约 19% 的行是注释），远高于项目平均水平。可能存在大量解释性注释或已注释掉的代码。

### 15. 版本号对齐

所有 crate 的 `version` 都是 `0.1.0`（通过 workspace），但 workspace 中的部分依赖版本是 hardcode 的。建议全部改为 workspace 依赖以统一管理。

---

## 📋 修复优先级路线图

```
Sprint 推荐修复顺序
┌──────────────┬──────────┬──────────┐
│ 优先级       │ 修复项   │ 工作量   │
├──────────────┼──────────┤──────────┤
│ P0 · 立即    │ #5 依赖版本不一致  │ 小     │
│              │ (rfd 0.14 vs 0.15) │        │
├──────────────┼──────────┤──────────┤
│ P1 · 本周    │ #1 mod.rs 迁移     │ 中     │
│              │ #2 gfx 重复结构    │ 小     │
│              │ #3 gfx workspace   │ 小     │
│              │ #4 workspace 重复  │ 小     │
├──────────────┼──────────┤──────────┤
│ P2 · 下周    │ #7 废弃代码清理    │ 小     │
│              │ #8 macos.rs 清理   │ 小     │
│              │ #10 废弃模块引用   │ 极小   │
├──────────────┼──────────┤──────────┤
│ P3 · 后续    │ #6 超长文件拆分    │ 大     │
│              │ #9 TODO 整理       │ 小     │
│              │ #11 dead_code 清理 │ 极小   │
└──────────────┴──────────┴──────────┘
```

---

## ✅ 亮点（做得好的地方）

1. **`unwrap()` 零违规**: `src/` 和 `crates/` 的生产代码中 **0 个 `.unwrap()` 调用**（仅测试代码中存在 9 处，属正常）
2. **Crate 命名规范一致**: 所有 crate 名称正确使用 `lumino-{module}` kebab-case 格式
3. **workspace 依赖机制已建立**: 大部分 crate 的元数据已正确继承 workspace
4. **unsafe 使用有注释**: 已知的 unsafe 块都有安全注释说明原因
5. **`panic!` 零使用**: 生产代码中无 `panic!` 调用
6. **模块化整体设计合理**: 7 个子 crate 的职责划分清晰

---

*报告结束。建议按 P0 → P1 → P2 → P3 顺序逐步清理。*
