//! NoteStore 集成操作（降级兼容层）
//!
//! 2026-08 单一权威源改造：`NoteStore`（SoA 冗余镜像）已删除。
//! 本模块保留全部公共 API 签名，内部降级为直接操作 `notes`（im::Vector），
//! 保证下游（ui-editor / ui）在重接到 MidiDocument 前零改动编译通过。
//!
//! 第二阶段 ChunkedNotes（MidiDocument 分块）接管后，本模块的
//! 批量热路径将迁往 document 侧，此兼容层随后删除。

mod access;
mod batch_edit;
mod batch_move;
mod delete;
mod insert;

#[cfg(test)]
mod tests;
