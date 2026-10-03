//! 批量写入阈值（曲线填充 / 形状 / 画刷工具共用）
//!
//! 一次性生成音符数**超过**该阈值时，写入改走批量归并
//! （`batch_insert_notes_with_ids` / `batch_insert_events_to_track_with_ids`）。
//!
//! 为什么必须分流：
//! - **逐音符插入**（`insert_note_with_id`）为每个音符记一条
//!   `NoteDeltaEvent::InsertAt`；渲染侧对每条事件发一条 `NoteEvent::Insert`，
//!   渲染线程执行 `GpuNoteBuffer::insert_at` → `move_range`，把该索引之后
//!   **全部**实例在 GPU 内搬移一次（含 staging buffer 创建 + `queue.submit`）。
//!   生成顺序（填充 = 按音高行、行内按 tick，跨行 tick 回绕）使每条插入都落在
//!   列表中部，`Σtail` 随音符数**超线性**增长（实测翻倍 N → Σtail ×2.4~3.3）。
//! - **批量归并**只发一次主轨结构重建（`TrackDelta`），成本 O(N+M)。
//!
//! 实测（`ui_fill_confirm_bench`，13288 音符）：逐音符 28.1 ms 写入 +
//! 13288 条 GPU 消息 + **6805 万实例搬移（≈1038 MB GPU 拷贝）**；
//! 批量 0.3 ms + 0 消息 + 0 搬移。
//!
//! 阈值取块大小量级（与 `ui_brush_stroke_bench` 标定一致）：小规模逐笔插入可
//! 保留 GPU 段内增量（比整段 `TrackDelta` 更省），大规模必须归并。

/// 批量归并写入阈值（音符数）
pub(crate) const BATCH_INSERT_THRESHOLD: usize = 2048;
