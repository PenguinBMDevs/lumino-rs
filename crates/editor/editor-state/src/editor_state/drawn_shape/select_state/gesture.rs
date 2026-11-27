//! 拖动与框选手势；`translate_shape` 是几何写回的唯一入口。
//!
//! 结构体字段仍在 `drawn_shape.rs`，这里只放方法。

use super::super::*;

impl ShapeSelectState {
    /// 图形是否属于给定音轨的**可交互域**（可见 / 可选中 / 可拖动）
    ///
    /// - 已确认图形：严格归其登记音轨；
    /// - 待确认镜像：几何尚未绑定音轨（owning 预览在任何非 Conductor 轨都渲染），
    ///   故在任何非 Conductor 轨都可交互——否则用户切轨后又会看到「看得见框不中」。
    pub(super) fn in_track(s: &DrawnShape, track: usize) -> bool {
        if s.pending.is_some() {
            track != 0
        } else {
            s.track == track
        }
    }

    /// 记录一次已应用的移动（绑定历史分组），并把几何 / 音符平移到位
    pub fn translate_shape(&mut self, id: u64, group: Option<u64>, dtick: f32, dkey: f32) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        let (dtick, dkey) = s.source.translate(dtick, dkey);
        for n in &mut s.notes {
            *n = n.translated(dtick, dkey);
        }
        if let Some(g) = group {
            s.moves.push(ShapeMove {
                group: g,
                dtick,
                dkey,
            });
        }
        true
    }

    // ── 拖拽移动 ─────────────────────────────────────────

    /// 开始拖拽**选中集合**（无选中时不进入拖拽态）
    ///
    /// 与拉框**互斥**：若此前处于拉框态则一并收敛（同一时刻只会有一个手势在跑，
    /// 避免 `moved`/`released` 的早退分支误把拉框事件当成拖拽来消费）。
    pub fn begin_drag(&mut self, start_tick: f32, start_key: f32) {
        if self.selected.is_empty() {
            return;
        }
        self.marquee = None;
        let bounds = self
            .selected_shapes()
            .filter_map(|s| s.source.bounds())
            .fold(None::<(f32, f32, f32)>, |acc, b| {
                Some(match acc {
                    None => (b.0, b.2, b.3),
                    Some((min_tick, min_key, max_key)) => {
                        (min_tick.min(b.0), min_key.min(b.2), max_key.max(b.3))
                    }
                })
            });
        self.drag = Some(ShapeDrag {
            start_tick,
            start_key,
            delta_tick: 0.0,
            delta_key: 0.0,
            bounds,
        });
    }

    /// 更新拖拽偏移（相对按下点）
    pub fn update_drag(&mut self, tick: f32, key: f32, snap: f32) {
        let Some(d) = self.drag.as_mut() else {
            return;
        };
        let snap = snap.max(1.0);
        let raw_dtick = tick - d.start_tick;
        // tick 对齐到吸附网格（与音符移动一致，保证落点干净）；key 取整
        let raw_dtick = (raw_dtick / snap).round() * snap;
        let raw_dkey = (key - d.start_key).round();
        // 按拖拽起点的几何边界整体夹取：越过 tick 0 时停住，不压扁图案（DEBT-20）
        let (delta_tick, delta_key) = match d.bounds {
            Some((min_tick, min_key, max_key)) => {
                clamp_translate_delta(raw_dtick, raw_dkey, min_tick, min_key, max_key)
            }
            None => (raw_dtick, raw_dkey),
        };
        d.delta_tick = delta_tick;
        d.delta_key = delta_key;
    }

    /// 当前拖拽状态
    pub fn drag(&self) -> Option<ShapeDrag> {
        self.drag
    }

    /// 是否正在拖拽图形
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 结束拖拽并返回 `(dtick, dkey)`；无实际位移时返回 `None`
    pub fn end_drag(&mut self) -> Option<(f32, f32)> {
        let d = self.drag.take()?;
        if d.delta_tick == 0.0 && d.delta_key == 0.0 {
            return None;
        }
        Some((d.delta_tick, d.delta_key))
    }

    // ── 空白拉框（框选） ───────────────────────────────────

    /// 从空白处起框（框选）；`additive` = 起框时按住修饰键 → 松手并入既有选中集
    ///
    /// 与拖拽**互斥**：若此前处于拖拽态则一并收敛（理由同 `begin_drag`）。
    pub fn begin_marquee(&mut self, tick: f32, key: f32, additive: bool) {
        self.drag = None;
        self.marquee = Some(ShapeMarquee {
            start_tick: tick,
            start_key: key,
            cur_tick: tick,
            cur_key: key,
            additive,
        });
    }

    /// 更新拉框当前点
    pub fn update_marquee(&mut self, tick: f32, key: f32) {
        if let Some(m) = self.marquee.as_mut() {
            m.cur_tick = tick;
            m.cur_key = key;
        }
    }

    /// 当前拉框矩形（供叠加层绘制）
    pub fn marquee(&self) -> Option<ShapeMarquee> {
        self.marquee
    }

    /// 是否正在拉框
    pub fn is_marqueeing(&self) -> bool {
        self.marquee.is_some()
    }

    /// 结束拉框并返回其矩形（供命中判定）
    pub fn end_marquee(&mut self) -> Option<ShapeMarquee> {
        self.marquee.take()
    }
}
