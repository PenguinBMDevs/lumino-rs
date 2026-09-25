//! 音符批量操作（NoteStore 兼容层清理后的残留子集）
//!
//! 2026-08 单一权威源改造：`NoteStore`（SoA 冗余镜像）已删除，本模块曾作为
//! 降级兼容层保留全部旧 API 签名。2026-09 死路径清理：无生产调用方的批量
//! 移动/删除/插入（含 `push_note`）全部删除——其中 `batch_move_notes` 还会
//! 在就地改 tick 后失序（破坏二分查询依赖的有序不变式），属「死且错」代码。
//!
//! 仅保留仍有生产调用的操作：
//! - `access::get_note_view`（只读视图）
//! - `batch_edit::apply_batch_edit`（批量编辑对话框）
//! - `insert::batch_insert_notes_with_ids` / `batch_insert_notes_to_track_with_ids`
//!   （粘贴/复制/协作落盘，返回已分配 id）

mod access;
mod batch_edit;
mod insert;
