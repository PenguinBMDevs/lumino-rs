//! Tempo 编辑统一入口
//!
//! 2026-08 事件驱动演进：`EditorData.tempo_points`（编辑态强类型）与
//! `document.tempo_changes`（保存/播放权威）保持一致。
//! 所有 tempo 写入必须经本模块方法，内部自动同步 `document.tempo_changes`，
//! 消除「保存/导出出口依赖 `apply_tempo_points` 补救」的脆弱设计
//! （UI 编辑权威源是 tempo_points，而保存链路只读 document.tempo_changes）。

use std::cmp::Ordering;

use super::EditorData;
use lumino_note_core::midi_types::TempoPoint;

impl EditorData {
    /// 整体替换 tempo 点并同步到 document（工程设置 / undo 恢复 / 重置）
    pub fn set_tempo_points(&mut self, points: Vec<TempoPoint>) {
        self.tempo_points = points;
        self.modified = true;
        self.sync_tempo_to_document();
    }

    /// 修改单个 tempo 点 BPM 并同步到 document（速度面板拖拽热路径）
    pub fn set_tempo_point(&mut self, index: usize, bpm: f64) -> bool {
        let Some(point) = self.tempo_points.get_mut(index) else {
            return false;
        };
        point.bpm = bpm;
        self.modified = true;
        self.sync_tempo_to_document();
        true
    }

    /// 添加 tempo 点（按 tick 排序 + 同 tick 去重）并同步到 document
    pub fn add_tempo_point(&mut self, tick: f32, bpm: f64) {
        self.tempo_points.push(TempoPoint { tick, bpm });
        self.tempo_points
            .sort_by(|a, b| a.tick.partial_cmp(&b.tick).unwrap_or(Ordering::Equal));
        self.tempo_points
            .dedup_by(|a, b| (a.tick - b.tick).abs() < f32::EPSILON);
        self.modified = true;
        self.sync_tempo_to_document();
    }

    /// 删除指定索引 tempo 点并同步到 document
    pub fn remove_tempo_point(&mut self, index: usize) -> bool {
        if index < self.tempo_points.len() {
            self.tempo_points.remove(index);
            self.modified = true;
            self.sync_tempo_to_document();
            true
        } else {
            false
        }
    }

    /// 从 document 刷新编辑态缓存（**装载文档后调用**；不置脏、不回写）
    ///
    /// 2026-10-20 审计 §三：此前「装载文档 → 读出 doc.tempo_changes 深拷贝 →
    /// set_tempo_points 立刻写回 doc」是一次毫无意义的往返（数据没变，白拷两遍）。
    /// 现在把「文档进入编辑器」这一处的缓存刷新收在这里：只读 document、只写缓存。
    /// 注意不能用 [`Self::set_tempo_points`]：那会 `modified = true` 并再次回写，
    /// 而装载文件不是用户编辑。
    pub fn adopt_tempo_cache_from_document(&mut self) {
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        let tempo_points: Vec<TempoPoint> = doc
            .tempo_changes
            .iter()
            .map(|&(tick, bpm)| TempoPoint {
                tick: tick as f32,
                bpm: bpm as f64,
            })
            .collect();
        let time_signatures = doc.time_signatures.clone();
        self.tempo_points = tempo_points;
        self.time_signatures = time_signatures;
    }

    /// 将 tempo_points 同步到 document.tempo_changes（document 为权威镜像）
    fn sync_tempo_to_document(&mut self) {
        if let Some(doc) = self.document.as_mut() {
            doc.tempo_changes = self
                .tempo_points
                .iter()
                .map(|tp| (tp.tick.max(0.0) as u32, tp.bpm as f32))
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_data_with_document() -> EditorData {
        // 复用测试辅助：构造含 document 的 EditorData（document 非 None）
        crate::EditorData::with_notes(0, &[])
    }

    #[test]
    fn test_set_tempo_points_syncs_document() {
        let mut data = make_data_with_document();
        data.set_tempo_points(vec![
            TempoPoint {
                tick: 0.0,
                bpm: 140.0,
            },
            TempoPoint {
                tick: 480.0,
                bpm: 90.5,
            },
        ]);
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.tempo_changes, vec![(0, 140.0), (480, 90.5)]);
    }

    #[test]
    fn test_set_tempo_point_syncs_document() {
        let mut data = make_data_with_document();
        assert!(data.set_tempo_point(0, 150.0));
        assert!(!data.set_tempo_point(99, 150.0), "越界索引应失败");
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.tempo_changes[0].1, 150.0);
    }

    #[test]
    fn test_add_tempo_point_sorts_and_dedups() {
        let mut data = make_data_with_document();
        data.set_tempo_points(vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }]);
        data.add_tempo_point(480.0, 100.0);
        data.add_tempo_point(480.0, 200.0); // 同 tick 去重，保留先插入
        assert_eq!(data.tempo_points.len(), 2);
        assert_eq!(data.tempo_points[0].tick, 0.0);
        assert_eq!(data.tempo_points[1].tick, 480.0);
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.tempo_changes.len(), 2);
    }

    #[test]
    fn test_remove_tempo_point_syncs_document() {
        let mut data = make_data_with_document();
        data.set_tempo_points(vec![
            TempoPoint {
                tick: 0.0,
                bpm: 120.0,
            },
            TempoPoint {
                tick: 480.0,
                bpm: 100.0,
            },
        ]);
        assert!(data.remove_tempo_point(0));
        assert_eq!(data.tempo_points.len(), 1);
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.tempo_changes.len(), 1);
    }

    #[test]
    fn test_set_time_signatures_syncs_document() {
        let mut data = make_data_with_document();
        data.set_time_signatures(vec![(0, 4, 4), (1920, 3, 4)]);
        assert_eq!(data.time_signatures, vec![(0, 4, 4), (1920, 3, 4)]);
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.time_signatures, vec![(0, 4, 4), (1920, 3, 4)]);
    }

    /// 装载文档后的缓存刷新：只读 document，**不**置脏、**不**回写
    /// （2026-10-20 审计 §三：取代此前「读出 → set_tempo_points 写回」的往返）
    #[test]
    fn test_adopt_tempo_cache_from_document() {
        let mut data = make_data_with_document();
        data.modified = false;
        {
            let doc = data.document.as_mut().expect("document 应存在");
            doc.tempo_changes = vec![(0, 90.0), (960, 180.0)];
            doc.time_signatures = vec![(0, 3, 4)];
        }

        data.adopt_tempo_cache_from_document();

        assert_eq!(data.tempo_points.len(), 2);
        assert_eq!(data.tempo_points[0].tick, 0.0);
        assert!((data.tempo_points[0].bpm - 90.0).abs() < 1e-9);
        assert_eq!(data.tempo_points[1].tick, 960.0);
        assert_eq!(
            data.time_signatures,
            vec![(0, 3, 4)],
            "拍号缓存同样从 document 刷新"
        );
        assert!(!data.modified, "装载刷新不得置脏（装载文件不是用户编辑）");
        // 不回写：document 内容保持不变
        let doc = data.document.as_ref().expect("document 应存在");
        assert_eq!(doc.tempo_changes, vec![(0, 90.0), (960, 180.0)]);
    }
}
