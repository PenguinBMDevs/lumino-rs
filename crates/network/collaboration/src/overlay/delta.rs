//! 区域增量检测
//!
//! 负责维护每个 `RegionCoord` 的快照，检测增量变化。
//! 每次比对只处理单个区域（不扫描全音轨），输出 DeltaResult。

use crate::overlay::types::{DeltaResult, RegionCoord, RegionSnapshot};
use crate::types::NoteBatchOperation;

/// 获取音符在区域内的指纹
///
/// 从 NoteBatchOperation 的 notes 中提取属于指定区域的所有音符指纹。
/// 指纹为 (tick, key, length) 三元组。
fn extract_fingerprints(
    operations: &[NoteBatchOperation],
    coord: &RegionCoord,
    ticks_per_group: u32,
) -> Vec<(u32, u16, u32)> {
    let tick_start = coord.time_group * ticks_per_group;
    let tick_end = tick_start + ticks_per_group;
    let track_start = (coord.track_group * 8) as usize;
    let track_end = track_start + 8;

    let mut fps = Vec::new();
    for op in operations {
        for note in &op.notes {
            // 只处理属于本区域的音符
            if note.track_index < track_start || note.track_index >= track_end {
                continue;
            }
            // tick 范围过滤
            let note_tick = note.tick as u32;
            if note_tick < tick_start || note_tick >= tick_end {
                continue;
            }
            fps.push((note_tick, note.key, note.length as u32));
        }
    }
    fps.sort_unstable_by_key(|f| (f.0, f.1, f.2));
    fps.dedup();
    fps
}

/// 区域增量检测器
///
/// 管理所有区域的快照，提供增量检测接口。
/// 每次检测只处理一个区域，不扫描全量数据。
pub struct RegionDeltaDetector {
    /// 当前快照：RegionCoord → RegionSnapshot
    snapshots: std::collections::HashMap<RegionCoord, RegionSnapshot>,
    /// 配置参数
    ticks_per_group: u32,
}

impl RegionDeltaDetector {
    pub fn new(ticks_per_group: u32) -> Self {
        Self {
            snapshots: std::collections::HashMap::new(),
            ticks_per_group,
        }
    }

    /// 获取或初始化区域的快照
    pub fn get_or_init_snapshot(
        &mut self,
        coord: &RegionCoord,
        operations: &[NoteBatchOperation],
        timestamp_ms: u64,
        active_user_count: u32,
    ) -> &RegionSnapshot {
        self.snapshots.entry(*coord).or_insert_with(|| {
            let fps = extract_fingerprints(operations, coord, self.ticks_per_group);
            RegionSnapshot::new(fps, timestamp_ms, active_user_count)
        })
    }

    /// 检测单个区域的增量
    ///
    /// 1. 获取当前区域的指纹
    /// 2. 比对上一次快照
    /// 3. 如果有变化，更新快照并返回 DeltaResult
    pub fn detect_delta(
        &mut self,
        coord: &RegionCoord,
        operations: &[NoteBatchOperation],
        timestamp_ms: u64,
        active_user_count: u32,
    ) -> DeltaResult {
        let current_fps = extract_fingerprints(operations, coord, self.ticks_per_group);

        let Some(prev_snapshot) = self.snapshots.get(coord) else {
            // 首次检测：初始化快照，无变化
            let snapshot =
                RegionSnapshot::new(current_fps.clone(), timestamp_ms, active_user_count);
            self.snapshots.insert(*coord, snapshot.clone());
            // 如果区域有音符但之前没有快照，也算变化（首次检测到内容）
            if current_fps.is_empty() {
                return DeltaResult::NoChange;
            }
            return DeltaResult::Changed(snapshot);
        };

        // 比对指纹
        if prev_snapshot.note_fingerprints == current_fps {
            return DeltaResult::NoChange;
        }

        if current_fps.is_empty() {
            // 区域被清空
            let snapshot = RegionSnapshot::new(current_fps, timestamp_ms, active_user_count);
            self.snapshots.insert(*coord, snapshot);
            return DeltaResult::Cleared;
        }

        // 有变化
        let new_snapshot = RegionSnapshot::new(current_fps, timestamp_ms, active_user_count);
        self.snapshots.insert(*coord, new_snapshot.clone());
        DeltaResult::Changed(new_snapshot)
    }

    /// 清除指定区域的快照（当区域合并到主贴图后）
    pub fn clear_region(&mut self, coord: &RegionCoord) {
        self.snapshots.remove(coord);
    }

    /// 获取区域快照数
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// 是否包含指定区域的快照
    pub fn has_snapshot(&self, coord: &RegionCoord) -> bool {
        self.snapshots.contains_key(coord)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{NoteAction, NoteBatchOperation, SyncNote};

    fn make_note(tick: f32, key: u16, length: f32, track: usize) -> SyncNote {
        SyncNote {
            id: format!("n_{}_{}", tick as u64, key),
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
            track_index: track,
        }
    }

    fn make_add_op(notes: Vec<SyncNote>) -> NoteBatchOperation {
        NoteBatchOperation {
            action: NoteAction::Add,
            notes,
            source_track: None,
            target_track: None,
            tick_offset: None,
            key_offset: None,
            timestamp: 1000,
        }
    }

    #[test]
    fn test_extract_fingerprints_in_region() {
        let coord = RegionCoord::new(0, 0); // track 0-7, time 0-30719
        let notes = vec![
            make_note(100.0, 60, 200.0, 0),   // track 0 ✓
            make_note(500.0, 64, 100.0, 1),   // track 1 ✓
            make_note(100.0, 60, 200.0, 10),  // track 10 ✗ (out of group)
            make_note(40000.0, 60, 100.0, 0), // tick 40000 ✗ (out of time group)
        ];
        let ops = vec![make_add_op(notes)];

        let fps = extract_fingerprints(&ops, &coord, 30720);
        assert_eq!(fps.len(), 2);
        assert!(fps.contains(&(100, 60, 200)));
        assert!(fps.contains(&(500, 64, 100)));
    }

    #[test]
    fn test_detect_delta_first_time_no_ops() {
        let mut detector = RegionDeltaDetector::new(30720);
        let coord = RegionCoord::new(0, 0);

        let detection_result = detector.detect_delta(&coord, &[], 1000, 0);
        assert_eq!(detection_result, DeltaResult::NoChange);
    }

    #[test]
    fn test_detect_delta_first_time_with_ops() {
        let mut detector = RegionDeltaDetector::new(30720);
        let coord = RegionCoord::new(0, 0);
        let ops = vec![make_add_op(vec![make_note(100.0, 60, 200.0, 0)])];

        let detection_result = detector.detect_delta(&coord, &ops, 1000, 1);
        assert!(matches!(detection_result, DeltaResult::Changed(_)));
    }

    #[test]
    fn test_detect_delta_no_change() {
        let mut detector = RegionDeltaDetector::new(30720);
        let coord = RegionCoord::new(0, 0);
        let ops = vec![make_add_op(vec![make_note(100.0, 60, 200.0, 0)])];

        // First detect: initial snapshot
        let _ = detector.detect_delta(&coord, &ops, 1000, 1);

        // Second detect with same data: no change
        let detection_result = detector.detect_delta(&coord, &ops, 2000, 1);
        assert_eq!(detection_result, DeltaResult::NoChange);
    }

    #[test]
    fn test_detect_delta_clear_region() {
        let mut detector = RegionDeltaDetector::new(30720);
        let coord = RegionCoord::new(0, 0);
        let ops_with = vec![make_add_op(vec![make_note(100.0, 60, 200.0, 0)])];

        let _ = detector.detect_delta(&coord, &ops_with, 1000, 1);

        // Now region is empty (all notes deleted)
        let detection_result = detector.detect_delta(&coord, &[], 2000, 1);
        assert_eq!(detection_result, DeltaResult::Cleared);
    }

    #[test]
    fn test_clear_region_removes_snapshot() {
        let mut detector = RegionDeltaDetector::new(30720);
        let coord = RegionCoord::new(0, 0);
        let ops = vec![make_add_op(vec![make_note(100.0, 60, 200.0, 0)])];

        let _ = detector.detect_delta(&coord, &ops, 1000, 1);
        assert!(detector.has_snapshot(&coord));

        detector.clear_region(&coord);
        assert!(!detector.has_snapshot(&coord));
        assert_eq!(detector.snapshot_count(), 0);
    }
}
