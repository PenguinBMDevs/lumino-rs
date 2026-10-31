//! 历史快照构造、`HistoryEntry` 应用与历史栈推入
//!
//! 由 `history.rs` 拆分而来：快照构造（`EditorSnapshot`）、历史条目的
//! 反向/正向应用、历史栈推入（快照 / `MoveOp` / `CreateOp`）与快照恢复。

use super::*;
use crate::editor_state::editor_data::cpu_onion_dirty::{CpuOnionChange, MAX_CPU_ONION_CHANGES};

impl EditorData {
    // ── 快照构造 ─────────────────────────────────────────────

    /// 构造当前状态的 EditorSnapshot（不带元数据）
    ///
    /// `ChunkedList::clone` 为 O(块数) 浅拷贝（块 Arc COW），快照与 document
    /// 物理共享未修改块——1600W 音符工程快照不再复制整轨数据。
    fn make_snapshot(&self) -> EditorSnapshot {
        self.make_snapshot_for_track(self.current_track)
    }

    /// 构造指定音轨当前状态的 EditorSnapshot（不带元数据）
    ///
    /// 用于 undo/redo 时生成 redo/undo 快照：当用户切轨后 undo 另一条音轨的编辑，
    /// redo 快照必须记录**被 undo 的音轨**而非当前视图音轨，否则 redo 无法恢复。
    pub(super) fn make_snapshot_for_track(&self, track_id: usize) -> EditorSnapshot {
        let notes = Arc::new(self.track_notes(track_id).clone());
        EditorSnapshot {
            notes,
            current_track: track_id,
            automation_lanes: self.automation_lanes.clone(),
            time_signatures: Some(self.time_signatures.clone()),
            tempo_points: Some(self.tempo_points.clone()),
            ..EditorSnapshot::new(
                Arc::new(lumino_midi_model::ChunkedList::new()),
                0,
                Vec::new(),
            )
        }
    }

    /// 从历史条目推断其影响的音轨（用于生成正确的 redo/undo 快照）。
    ///
    /// Snapshot：current_track 即被快照的音轨；
    /// Operation / Create：取第一个 op 的 track_id（常规单轨操作）。
    pub(super) fn affected_track_of_history_entry(
        entry: &lumino_note_core::history::HistoryEntry,
    ) -> Option<usize> {
        match entry {
            lumino_note_core::history::HistoryEntry::Snapshot(s) => Some(s.current_track),
            lumino_note_core::history::HistoryEntry::Operation(op) => {
                op.ops.first().map(|o| o.track_id as usize)
            }
            lumino_note_core::history::HistoryEntry::Create(entry) => {
                entry.ops.first().map(|o| o.track_id as usize)
            }
        }
    }

    // ── 向后兼容的 push / undo / redo ───────────────────────

    /// 将当前状态快照推入历史记录（O(lane 数) Arc clone，真共享）
    ///
    /// 向后兼容版本：op_kind = Other，每个 push 独立 group。
    /// **新代码应使用 `push_history_with_op_kind` 或 `push_history_mergeable`**
    /// 以获得逻辑撤销链 / 合并窗口能力。
    pub fn push_history(&mut self) {
        self.commit_history_pending();
        self.history.push(self.make_snapshot());
    }

    /// 推入带 op_kind 的快照（不合并，但分配独立 group_id）
    ///
    /// 适用：NoteMove / NoteDelete / NoteTransform / VelocityEdit / AutomationEdit / Recording
    /// 这些操作不走合并窗口，但需要 group_id 以支持未来扩展（如批量操作的逻辑分组）。
    pub fn push_history_with_op_kind(&mut self, op_kind: OpKind) {
        self.commit_history_pending();
        self.history
            .push_with_op_kind(self.make_snapshot(), op_kind);
    }

    /// 推入可合并的快照（仅 `OpKind::NoteCreate` 等可合并类型）
    ///
    /// 适用：Pencil 绘制连续放置音符。
    /// 合并规则：栈顶 op_kind 相同 + 在合并窗口内 + 未超 entry 上限 → 合并。
    /// 返回 `true` 表示合并到上一条，`false` 表示新增/分割。
    pub fn push_history_mergeable(&mut self, op_kind: OpKind) -> bool {
        self.commit_history_pending();
        self.history.push_mergeable(self.make_snapshot(), op_kind)
    }

    /// 推入 NoteMove 操作日志
    ///
    /// 拖动提交路径使用：用轻量 MoveOp 替代完整快照。
    pub fn push_move_op(&mut self, ops: Vec<MoveOp>) -> u64 {
        self.commit_history_pending();
        self.history.push_move_op(ops)
    }

    /// 推入音符创建操作日志（NoteCreate 增量、极简化）
    ///
    /// 铅笔绘制路径使用：每 op 仅 20 字节，替代整轨快照克隆——
    /// 1600W 音符工程在合并窗口内连续绘制不再复制音符数据。
    /// 返回 `true` 表示合并到上一条（300ms 窗口内）。
    pub fn push_note_create(&mut self, ops: Vec<CreateOp>) -> bool {
        self.commit_history_pending_for_create(&ops);
        self.history.push_note_create(ops)
    }

    // ── 历史记录条目应用 ─────────────────────────────────────
    /// 根据 HistoryEntry 类型应用撤销/重做（删加语义，按值，无全扫）。
    ///
    /// `snapshot_changes` / `snapshot_full` 只对 `Snapshot` 条目有效：由
    /// undo/redo 从「待挂变更或条目自身变更」解析后传入；Operation/Create
    /// 的精确 old/new 直接从 op 构造。回放期间内部写路径不污染历史队列。
    pub(super) fn apply_history_entry(
        &mut self,
        entry: HistoryEntry,
        inverse: bool,
        snapshot_changes: Vec<lumino_note_core::history::SnapshotNoteChange>,
        mut ranges: Vec<(u32, u32)>,
        snapshot_full: bool,
    ) {
        // `apply_move_ops` / `apply_create_ops` 内部会调用
        // `mark_track_notes_changed_for`，这是覆盖语义。多步历史跳转在同一帧
        // 连续 apply 时，上一步的脏集合会在这里被中间覆盖；先保存并在末尾并集。
        let previous_onion = self.onion_dirty_tracks.take();
        let previous_main_struct = self.main_track_struct_dirty;

        self.history_replay = true;
        let mut replay = Vec::new();
        let mut replay_full = false;
        // 清空可能残留的旧 note_delta_events，防止其在新 current_track 下被误应用到
        // 错误音轨（原 mark_tracks_changed_after_history 的防跨轨误用意图保留于此）。
        self.note_delta_events.clear();
        // 协作同步开关：关闭时跳过全部 `pending_collab_*` 广播数据构建
        // （快照分支的整轨对账/移动分支的逐音符记录均为 O(N)/O(K)，
        // 未连接协作时被消费端短路丢弃，纯浪费）。
        let collab_sync = self.collab_sync_enabled;
        match entry {
            HistoryEntry::Snapshot(s) => {
                let track = s.current_track;
                if snapshot_full && ranges.is_empty() {
                    let lo = s.notes.first().map_or(0, |note| note.start_tick);
                    let hi = s
                        .notes
                        .iter()
                        .map(|note| note.end_tick.max(note.start_tick.saturating_add(1)))
                        .max()
                        .unwrap_or(lo);
                    ranges.push((lo, hi.max(lo.saturating_add(1))));
                }
                if collab_sync {
                    // 协作对账（按值多重集差分，无 ID）：
                    // Snapshot 仅用于自动化/拍号等非音符高频路径（音符移动/创建已走
                    // Move/Create 删加日志，O(K log N) 无全扫）。此处整轨值差分 O(N)
                    // 仅在“协作开启 + 快照回放”时触发，属快照语义固有成本，非查询全扫。
                    // 值键用整数（start,end,key,vel,chan）满足 Eq+Hash，无浮点哈希问题。
                    type SyncKey = (u32, u8, u32, u8, u8);
                    type SyncTuple = (f32, u16, f32, u8, u8);
                    fn to_key(n: &lumino_midi_model::NoteEvent) -> SyncKey {
                        (n.start_tick, n.key, n.end_tick, n.velocity, n.channel)
                    }
                    fn to_tuple(n: &lumino_midi_model::NoteEvent) -> SyncTuple {
                        (
                            n.start_tick as f32,
                            n.key as u16,
                            n.length() as f32,
                            n.velocity,
                            n.channel,
                        )
                    }
                    let mut before_counts: HashMap<SyncKey, (usize, SyncTuple)> = HashMap::new();
                    // 功效说明：此处的整轨迭代为快照差分所必需（快照即整轨语义），
                    // 非“定位查询全扫”；音符高频路径（Move/Create）已无任何全扫。
                    for n in self.track_notes(track).iter() {
                        let k = to_key(n);
                        let t = to_tuple(n);
                        before_counts
                            .entry(k)
                            .and_modify(|e| e.0 += 1)
                            .or_insert((1, t));
                    }
                    let after_notes = s.notes.clone(); // Arc 浅拷，O(块)
                    self.apply_snapshot(*s); // 恢复 pre-op
                    let mut after_counts: HashMap<SyncKey, (usize, SyncTuple)> = HashMap::new();
                    for n in after_notes.iter() {
                        let k = to_key(n);
                        let t = to_tuple(n);
                        after_counts
                            .entry(k)
                            .and_modify(|e| e.0 += 1)
                            .or_insert((1, t));
                    }
                    let mut sync = Vec::new();
                    // 差分：after 多出 → 加，before 多出 → 删（按份数）。
                    for (k, (c_after, t)) in &after_counts {
                        let c_before = before_counts.get(k).map(|e| e.0).unwrap_or(0);
                        for _ in c_before..*c_after {
                            sync.push((true, t.0, t.1, t.2, t.3, t.4, track));
                        }
                    }
                    for (k, (c_before, t)) in &before_counts {
                        let c_after = after_counts.get(k).map(|e| e.0).unwrap_or(0);
                        for _ in c_after..*c_before {
                            sync.push((false, t.0, t.1, t.2, t.3, t.4, track));
                        }
                    }
                    self.pending_collab_transform_sync = sync;
                } else {
                    self.apply_snapshot(*s);
                }
                self.mark_tracks_changed_after_history(HashSet::from([track]));
                replay_full = snapshot_full;
                replay.extend(snapshot_changes.into_iter().map(|change| CpuOnionChange {
                    track: change.track,
                    old: if inverse { change.new } else { change.old },
                    new: if inverse { change.old } else { change.new },
                }));
            }
            HistoryEntry::Operation(op) => {
                // 对 OperationEntry：undo 时传入 inverse=true 删 moved 加回 originals；
                // redo 时 inverse=false 删 originals 加 moved（删加语义，按值）。
                let affected: HashSet<usize> = op.ops.iter().map(|o| o.track_id as usize).collect();
                let _ = self.apply_move_ops(&op.ops, inverse, self.max_key_for_move_op());
                if collab_sync {
                    // 构造协作广播（按值 + 偏移，操作者标识由信封承载）：
                    // op 恒为前向（originals + delta），方向由 inverse 标志决定。
                    // undo：ref = moved（O+D），off = -D；redo：ref = O，off = +D。
                    // 对端按“删 ref + 加 ref+off”执行，与本地删加一致。
                    let mut sync = Vec::new();
                    for m in &op.ops {
                        let track = m.track_id as usize;
                        let moved = m.moved_notes(self.max_key_for_move_op());
                        if inverse {
                            for n in &moved {
                                sync.push((
                                    n.start_tick as f32,
                                    n.key as u16,
                                    -(m.delta_tick as f32),
                                    -m.delta_key,
                                    track,
                                ));
                            }
                        } else {
                            for n in &m.originals {
                                sync.push((
                                    n.start_tick as f32,
                                    n.key as u16,
                                    m.delta_tick as f32,
                                    m.delta_key,
                                    track,
                                ));
                            }
                        }
                    }
                    self.pending_collab_move_sync = sync;
                }
                // Move/Create 历史统一走累积 helper（见 `c7d05a88`：内部写路径的
                // 覆盖语义会让多步跳转丢掉上一步的脏集合）；CPU 洋葱皮的
                // replay 变更与区间另由 `record_cpu_onion_replay_changes` 承担。
                self.mark_tracks_changed_after_history(affected);
                let max_key = self.max_key_for_move_op();
                for op in &op.ops {
                    let moved = op.moved_notes(max_key);
                    if moved.len() != op.originals.len() {
                        for note in op.originals.iter().chain(moved.iter()) {
                            super::ranges::push_union(
                                &mut ranges,
                                note.start_tick,
                                note.end_tick.max(note.start_tick.saturating_add(1)),
                            );
                        }
                        replay_full = true;
                        continue;
                    }
                    for (original, moved) in op.originals.iter().zip(moved) {
                        if *original == moved {
                            continue;
                        }
                        super::ranges::push_union(
                            &mut ranges,
                            original.start_tick,
                            original.end_tick.max(original.start_tick.saturating_add(1)),
                        );
                        super::ranges::push_union(
                            &mut ranges,
                            moved.start_tick,
                            moved.end_tick.max(moved.start_tick.saturating_add(1)),
                        );
                        if replay.len() < MAX_CPU_ONION_CHANGES {
                            replay.push(CpuOnionChange {
                                track: op.track_id as usize,
                                old: if inverse {
                                    Some(moved)
                                } else {
                                    Some(*original)
                                },
                                new: if inverse {
                                    Some(*original)
                                } else {
                                    Some(moved)
                                },
                            });
                        }
                    }
                }
            }
            HistoryEntry::Create(entry) => {
                let affected: HashSet<usize> =
                    entry.ops.iter().map(|o| o.track_id as usize).collect();
                let _ = self.apply_create_ops(&entry.ops, inverse);
                if collab_sync {
                    // 构造协作广播（按值）：undo→删除(false)，redo→添加(true)。
                    let is_added = !inverse;
                    let mut sync = Vec::new();
                    for op in &entry.ops {
                        let note = op.note;
                        sync.push((
                            note.start_tick as f32,
                            note.key as u16,
                            note.length() as f32,
                            note.velocity,
                            note.channel,
                            op.track_id as usize,
                            is_added,
                        ));
                    }
                    self.pending_collab_create_sync = sync;
                }
                self.mark_tracks_changed_after_history(affected);
                for op in &entry.ops {
                    super::ranges::push_union(
                        &mut ranges,
                        op.note.start_tick,
                        op.note.end_tick.max(op.note.start_tick.saturating_add(1)),
                    );
                    if replay.len() < MAX_CPU_ONION_CHANGES {
                        replay.push(CpuOnionChange {
                            track: op.track_id as usize,
                            old: inverse.then_some(op.note),
                            new: (!inverse).then_some(op.note),
                        });
                    }
                }
            }
        }
        self.history_replay = false;
        if ranges.is_empty() {
            for change in &replay {
                if let Some((lo, hi)) =
                    super::super::cpu_onion_dirty::note_change_range(change.old, change.new)
                {
                    ranges.push((lo, hi));
                }
            }
        }
        if !ranges.is_empty() {
            replay_full = false;
        }
        self.record_cpu_onion_replay_changes(&replay, &ranges, replay_full);

        if let Some(previous) = previous_onion {
            self.onion_dirty_tracks = Some(match self.onion_dirty_tracks.take() {
                Some(current) => previous.union(&current).copied().collect(),
                None => previous,
            });
        }
        self.main_track_struct_dirty |= previous_main_struct;
    }

    /// 撤销/重做后标记受影响的音轨并清理过期增量事件。
    ///
    /// 与常规编辑不同，undo/redo 可能作用于非当前视图音轨。本方法：
    /// - 精确记录受影响的音轨集合，供洋葱皮层走 `TrackDelta` 增量同步。
    /// - 若当前音轨也在受影响集合内，由于 undo/redo 入口未记录主音轨段内
    ///   增量事件，标记**主轨段重建**（单轨 `TrackDelta`，非全量会话兜底）。
    /// - 清空可能残留的旧 `note_delta_events`，防止其在新 `current_track` 下
    ///   被误应用到错误音轨。
    fn mark_tracks_changed_after_history(&mut self, affected_tracks: HashSet<usize>) {
        self.note_delta_events.clear();
        // 多步历史跳转会在同一帧内连续调用本方法。这里必须**累积**受影响轨：
        // 覆盖赋值会让「步 1 改当前轨、步 2 改其他轨」的最终 flags 只剩最后一步，
        // 当前轨段既不重建也无增量事件（CPU 文档正确、GPU 旧索引/旧顺序）。
        self.onion_dirty_tracks = Some(match self.onion_dirty_tracks.take() {
            Some(previous) => previous.union(&affected_tracks).copied().collect(),
            None => affected_tracks.clone(),
        });
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
        // 当前轨整轨替换 → 主轨段重建；其余受影响音轨由洋葱皮 Delta 同步。
        // 多步跳转时必须保留任一步产生的 true，不能覆盖回 false。
        self.main_track_struct_dirty |= affected_tracks.contains(&self.current_track);
        self.note_delta_dirty = false;
    }

    /// 应用快照到当前状态（undo / redo 后调用）
    fn apply_snapshot(&mut self, snapshot: EditorSnapshot) {
        // 音符快照写回 document（整轨替换，单一权威源）。
        // O(块数) 浅拷贝：直接共享快照块 Arc，不复制音符数据。
        //
        // 注意：快照的 current_track 仅用于定位该快照音符属于哪条音轨，
        // **不得恢复为当前视图音轨**。undo/redo 只应恢复数据，不应改变用户
        // 当前正在查看的音轨；否则切轨后按 Ctrl+Z 会跳回上一条被编辑的音轨。
        self.replace_track_notes_chunked(snapshot.current_track, snapshot.notes.as_ref());
        self.automation_lanes = snapshot.automation_lanes.clone();
        // 经统一入口同步：lane 是编辑态，document.control_events 是 CC/PB 唯一权威。
        // 不同步的话 undo 后保存/导出仍会写出被撤销的曲线（2026-10-17 整改）。
        self.sync_automation_to_document();
        if let Some(v) = snapshot.time_signatures {
            // 经统一入口恢复，保证 document.time_signatures 同步（保存链路读到最新）
            self.set_time_signatures(v);
        }
        if let Some(v) = snapshot.tempo_points {
            // 经统一入口恢复，保证 document.tempo_changes 同步（保存链路读到最新）
            self.set_tempo_points(v);
        }
    }
}
