//! Editor 编辑状态判定与提交（pending 拖动/复制、异步提交、当前编辑收尾）
//!
//! 从 `impls/editor_impl.rs` 抽出，按职责分片：
//! `history`（历史入栈）、`state`（编辑态查询）、`drag` / `copy`（pending 提交）、
//! `async_commit`（异步结果）、`current_edit`（收尾）、`helpers`（纯函数）。

mod async_commit;
mod copy;
mod current_edit;
mod drag;
mod helpers;
mod history;
mod state;
