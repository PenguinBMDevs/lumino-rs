//! `ShapeSelectState` 的按职责 impl 分片。
//!
//! 结构体字段仍在 `drawn_shape.rs`；拆分只按「选择 / pending 镜像 /
//! 撤销组 / 手势」分组，不改字段、不改调用点。

mod gesture;
mod history;
mod pending;
mod selection;
