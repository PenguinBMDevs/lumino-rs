//! Automation lane → `MidiDocument.control_events` 单一权威同步
//!
//! ## 背景（2026-10-17 数据源冗余审计 §一）
//!
//! `doc.control_events` 是 CC/PB 的保存 / 导出 / 非当前轨播放权威，但编辑只写
//! `automation_lanes`（加载时从 document 深拷贝出来的副本）且**从不回写**：
//! 实时播放读 lane、保存与导出读 document → 用户在 CC/弯音面板画的曲线
//! 「**听得到、存不下来**」。
//!
//! 本模块是该问题的**唯一**同步入口，与仓库既有的 tempo / 拍号模式同构
//! （`tempo.rs::sync_tempo_to_document`、`accessors/write.rs::set_time_signatures`）：
//! - `automation_lanes`：编辑态（曲线 `shape` / 贝塞尔柄 + UI 与实时播放采样）
//! - `doc.control_events`：CC/PB 的**唯一权威**（PC / 触后亦在此）
//!
//! ## 契约（改 lane 的人必须读）
//!
//! 任何改写 `automation_lanes` 事件的路径都必须使 document 跟上：
//! - `apply_automation_edit`：**已内联**（方法末尾统一同步，见 `automation.rs`）；
//! - undo / redo 恢复 lane 快照：**已接入**（`history/apply.rs::apply_snapshot`）；
//! - `import_control_events_from_document`：无需同步（lane 本就由 document 派生，同源）；
//! - `channel.rs` 通道变更：无需同步（它同时改 document 全 kind 与 lane.channel，两侧一致）；
//! - 新增任何写 lane 事件的路径：**必须**调用 [`EditorData::sync_automation_to_document`]。
//!
//! ## 覆盖粒度与边界
//!
//! | lane target | document 处理 |
//! |---|---|
//! | `CC { controller }` | 覆盖该 `(track, kind=0, controller)` 的全部事件 |
//! | `PitchBend` | 覆盖该 `(track, kind=2)` 的全部事件 |
//! | `Rpn` / `Nrpn` | **不落 document**（无对应 kind；实时播放由 `AutomationLane::to_midi_cc_events` 展开为 CC 序列，导出行为与整改前一致） |
//! | kind 1=PC / 3=通道触后 / 4=复音触后 | 原样保留，不受影响 |
//!
//! ## 同 tick 顺序契约
//!
//! 与 `project/conversion.rs`（「同 tick 组装顺序 CC → PC → PB：RPN 选择/DataEntry
//! 必须先于同 tick 的 PB 生效」）及 `document_build` 一致：重建时按 kind 分桶
//! （0 CC → 1 PC → 2 PB → 3 触后 → 4 复音触后 → 其余）拼接后再**稳定**按 tick
//! 排序，保持桶内相对顺序。
//!
//! ## 性能口径
//!
//! 全量重建 O(control_events)，与既有 `apply_automation_edit` 的
//! `sort_by_key` + `recompute_auto_handles`（O(M log M) / O(M)）同量级，
//! 不会成为新的量级瓶颈（黑乐谱实测 45.8 万控制事件为毫秒级）。
//! 若将来需要亚毫秒级拖拽，可改为「二分定位同 tick + 局部替换」的增量实现；
//! 当前刻意选择全量重建：**正确性优先，单点实现易验证**。

use lumino_midi_model::{ChunkedList, PackedControlEvent};
use lumino_note_core::automation::{AutomationEdit, AutomationTarget};

use super::EditorData;

/// 该编辑是否改变 **document 可见内容**（`(tick, value)`）。
///
/// `CycleShape` / `UpdateHandles` 只改曲线形状与贝塞尔柄——document 里没有对应
/// 字段（导出按锚点 Step 采样），属纯编辑态装饰，因此**不触发**同步：
/// 拖控制柄是高频热路径（每帧多次），无谓的全量重建必须避免。
/// 其余变体都改变 tick/value，必须同步。
pub(crate) fn edit_affects_document(edit: &AutomationEdit) -> bool {
    matches!(
        edit,
        AutomationEdit::Add { .. }
            | AutomationEdit::Move { .. }
            | AutomationEdit::Delete { .. }
            | AutomationEdit::Clear { .. }
    )
}

/// `PackedControlEvent.kind`：控制变更
const KIND_CC: usize = 0;
/// `PackedControlEvent.kind`：弯音
const KIND_PB: usize = 2;
/// 已知 kind 数（0=CC / 1=PC / 2=PB / 3=通道触后 / 4=复音触后）
const KNOWN_KINDS: usize = 5;
/// 分桶数 = 已知 kind + 1 个「其余 kind」兜底桶（保持未知 kind 的相对顺序与末位）
const KIND_BUCKETS: usize = KNOWN_KINDS + 1;

impl EditorData {
    /// 将 `automation_lanes`（编辑态）同步进 `document.control_events`（唯一权威）。
    ///
    /// - 无 document 或**完全没有 lane** 时为空操作（后者保证未使用自动化的文档
    ///   逐字节不变，避免无谓重排同 tick 事件顺序）；
    /// - 幂等：连续调用结果一致。
    pub fn sync_automation_to_document(&mut self) {
        if self.automation_lanes.is_empty() {
            return;
        }

        // ── 1. 先在只读借用下生成「lane 视角」的事件与覆盖集合 ──────────────
        // covered_cc: (track, controller) 由 lane 说了算的 CC
        // covered_pb: track 由 lane 说了算的 PB
        let mut covered_cc: Vec<(u16, u8)> = Vec::new();
        let mut covered_pb: Vec<u16> = Vec::new();
        let mut lane_events: Vec<PackedControlEvent> = Vec::new();

        for lane in &self.automation_lanes {
            match &lane.target {
                AutomationTarget::CC { controller } => {
                    covered_cc.push((lane.track, *controller));
                    for ev in &lane.events {
                        lane_events.push(PackedControlEvent::control_change(
                            ev.tick,
                            lane.track,
                            lane.channel,
                            *controller,
                            ev.value.min(127) as u8,
                        ));
                    }
                }
                AutomationTarget::PitchBend => {
                    covered_pb.push(lane.track);
                    for ev in &lane.events {
                        lane_events.push(PackedControlEvent::pitch_bend(
                            ev.tick,
                            lane.track,
                            lane.channel,
                            ev.value.min(16383),
                        ));
                    }
                }
                // RPN/NRPN 无对应 kind：播放时展开为 CC 序列，不入 document
                AutomationTarget::Rpn { .. } | AutomationTarget::Nrpn { .. } => {}
            }
        }

        let Some(doc) = self.document.as_mut() else {
            return;
        };

        // ── 2. 按 kind 分桶：保留未被 lane 覆盖的既有事件，再并入 lane 事件 ──
        let mut buckets: [Vec<PackedControlEvent>; KIND_BUCKETS] =
            std::array::from_fn(|_| Vec::new());

        for ev in doc.control_events.iter() {
            // 注意：`PackedControlEvent` 是 packed 结构，字段引用可能未对齐
            // （rustc E0793「reference to field of packed struct is unaligned」），
            // 因此一律先按值拷出字段再使用，不做 `&ev.field`。
            let kind = ev.kind as usize;
            let track = ev.track;
            let covered = match kind {
                KIND_CC => {
                    let (controller, _) = ev.as_control_change();
                    covered_cc.iter().any(|(covered_track, covered_ctrl)| {
                        *covered_track == track && *covered_ctrl == controller
                    })
                }
                KIND_PB => covered_pb.contains(&track),
                _ => false,
            };
            if !covered {
                buckets[kind.min(KNOWN_KINDS)].push(*ev);
            }
        }

        for ev in lane_events {
            let kind = ev.kind as usize;
            buckets[kind.min(KNOWN_KINDS)].push(ev);
        }

        // ── 3. 按 kind 顺序拼接 → 稳定按 tick 排序 → 重建分块容器 ────────────
        let mut out: Vec<PackedControlEvent> =
            Vec::with_capacity(buckets.iter().map(Vec::len).sum::<usize>());
        for mut bucket in buckets {
            out.append(&mut bucket);
        }
        // 稳定排序：同 tick 保持「CC → PC → PB → 触后 → 其余」相对顺序
        out.sort_by_key(|e| e.tick);
        doc.control_events = ChunkedList::from_sorted(out);
    }

    /// 统计 document 中指定 `(track, kind, controller)` 的事件数（同步测试与诊断用）。
    ///
    /// `controller` 仅对 `kind = 0`(CC) 生效；其余 kind 传 `None`。
    #[cfg(test)]
    pub(crate) fn count_document_control_events(
        &self,
        track: u16,
        kind: usize,
        controller: Option<u8>,
    ) -> usize {
        let Some(doc) = self.document.as_ref() else {
            return 0;
        };
        doc.control_events
            .iter()
            .filter(|ev| {
                ev.kind as usize == kind
                    && ev.track == track
                    && controller.is_none_or(|c| ev.as_control_change().0 == c)
            })
            .count()
    }

    /// 取 document 中指定 `(track, kind, controller)` 的 `(tick, value)` 列表（按 tick 升序）。
    #[cfg(test)]
    pub(crate) fn document_control_values(
        &self,
        track: u16,
        kind: usize,
        controller: Option<u8>,
    ) -> Vec<(u32, u16)> {
        let Some(doc) = self.document.as_ref() else {
            return Vec::new();
        };
        doc.control_events
            .iter()
            .filter(|ev| {
                ev.kind as usize == kind
                    && ev.track == track
                    && controller.is_none_or(|c| ev.as_control_change().0 == c)
            })
            .map(|ev| {
                let value = match ev.kind as usize {
                    KIND_CC => ev.as_control_change().1 as u16,
                    // 与 `extract.rs::extract_passthrough_events` 同口径还原 0..16383
                    KIND_PB => {
                        let bend = (ev.as_pitch_bend() * 8192.0).round() as i32 + 8192;
                        bend.clamp(0, 16383) as u16
                    }
                    _ => 0,
                };
                (ev.tick, value)
            })
            .collect()
    }
}
