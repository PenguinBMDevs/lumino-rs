//! 工具栏批量操作的**视图仲裁闸门**（钢琴卷帘 ⇄ 工程走带的作用域隔离）
//!
//! # 为什么需要它（P1-4 根因）
//!
//! 两个编辑视图各有独立选区：
//! - 钢琴卷帘：`interaction.selected_notes`（**单轨**音符索引位图）
//! - 工程走带：`data.arrange_selection`（**跨轨**矩形 / 冻结精确集）
//!
//! 而量化 / 翻转 / 移调 / 连奏 / 分割合并这批批量操作**全部只作用于卷帘选区**。
//! 用户在走带界面点这些按钮时，走带选区非空、卷帘选区通常是空的，于是：
//!
//! | 操作 | 空卷帘选区时的原行为 | 后果 |
//! |---|---|---|
//! | **量化** | `selected.is_empty()` → 收集 `(0..note_count)` | **静默量化当前轨全部音符**，而用户在走带上看不到这个作用域 |
//! | 垂直/水平翻转 | 多数路径 early-return | 点了没反应（无提示） |
//! | 移调 / 连奏 | early-return | 点了没反应（无提示） |
//! | 分割 | early-return | 点了没反应（无提示） |
//! | 合并 | 命中无选区默认值 | 行为不可预期 |
//!
//! 量化那条是**数据破坏级**：一个按钮点击作用域覆盖整轨，用户却以为在处理框选区。
//!
//! # 本模块的修法
//!
//! 在每个批量操作入口先过 [`ToolbarHandler::arrangement_batch_gate`]：
//! 走带模式下**无走带选区即拒绝执行**并打明确日志，**绝不退化为作用卷帘选区
//! 或整轨**。跨轨实现（走带版量化 / 翻转 / 移调 / 连奏 / 分割合并）属 P2 能力
//! 补齐，不在本次修复范围——但**误伤必须先堵死**。
//!
//! 已有走带实现的「变速」不走本闸门：它在 `handle_toolbar_speed_change` 内
//! 自行做了 `is_arrangement_mode()` 分流（作用于 `arrange_selection`）。

use super::super::ToolbarHandler;
use crate::root::Root;

impl ToolbarHandler {
    /// 走带模式下的批量操作前置闸门：走带选区为空时**拒绝执行**并返回 `false`。
    ///
    /// 返回 `true` 表示放行（钢琴卷帘模式，或走带模式且确有走带选区）。
    ///
    /// `op` 仅用于日志，让用户/开发者能看出是哪个操作被拒。
    pub(super) fn arrangement_batch_gate(root: &Root, op: &str) -> bool {
        if !root.is_arrangement_mode() {
            return true;
        }
        if root.editor.editor_state.data.arrange_selection.is_empty() {
            tracing::warn!(
                "Root: 工程走带模式下的「{op}」被拒绝——请先在走带视图框选音符。\
                 该操作的走带版尚未接线，绝不允许回退到卷帘选区或整轨（否则量化会静默量化整轨）"
            );
            return false;
        }
        tracing::debug!(
            "Root: 工程走带模式下的「{op}」有走带选区，但该操作尚未接线走带路径（当前无效果）"
        );
        true
    }
}
