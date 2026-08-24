//! 画刷工具绘制逻辑
//!
//! 画刷 = 在卷帘上长按跟随鼠标轨迹落笔；每次进入新网格单元，按当前粗细度纵向铺开
//! 一层「色块」：level k 的音符 key = 底键 + k（上限 255），写入该层配置/默认音轨。
//! 音符长度 = 当前吸附精度（`view.snap_precision`），力度 = 默认值。
//!
//! 多轨分配：`BrushConfig.tracks[level]` 显式指定则用之；`None` 则按默认规则——
//! 从当前音轨起沿普通音轨（排除 Conductor=0）序行走，每个 level 一条不同音轨。

use crate::EditState;
use crate::Note;
use crate::{Editor, HitType};
use iced_core::Point;
use lumino_ui_core::constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};

impl Editor {
    /// 处理画刷工具按下：命中已有音符则进入编辑，否则开始一笔绘制
    pub(crate) fn handle_brush_pressed(
        &mut self,
        pos: Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        // 命中已有音符时复用铅笔的「编辑已有音符」逻辑，否则开始画刷落笔
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
            return;
        }
        self.start_brush_stroke(snapped_tick, key);
    }

    /// 开始一笔画刷绘制（落笔首格）
    pub(crate) fn start_brush_stroke(&mut self, snapped_tick: f32, key: u16) {
        self.brush_last_cell = Some((snapped_tick, key));
        // 整笔作为一次撤销单元（历史合并窗口内连续插入归并）
        self.push_history();
        self.insert_brush_block(snapped_tick, key);
        self.editor_state.interaction.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
    }

    /// 鼠标移动时若处于画刷笔触中，跟随轨迹落笔（每进入新格落一次）
    pub(crate) fn handle_brush_moved(&mut self, pos: Point) {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
        let snapped_tick = self.snap_tick(tick);
        let cell = (snapped_tick, key);
        if let Some(last) = self.brush_last_cell
            && last.0 == cell.0
            && last.1 == cell.1
        {
            return; // 同一格，不重复落笔
        }
        self.brush_last_cell = Some(cell);
        self.insert_brush_block(snapped_tick, key);
    }

    /// 在 (tick, key) 处落一笔纵向色块
    fn insert_brush_block(&mut self, tick: f32, key: u16) {
        let thickness = self.brush.thickness as usize;
        if thickness == 0 {
            return;
        }
        // 音符长度 = 当前吸附精度（一个网格单元）
        let length = self.editor_state.view.snap_precision.max(1.0);
        let mut affected = std::collections::HashSet::new();
        for level in 0..thickness {
            let k = key.saturating_add(level as u16);
            if k > 255 {
                break;
            }
            let doc_track = self.brush_track_for_level(level);
            let note = Note::from_raw(tick, k, length, DEFAULT_NOTE_VELOCITY, DEFAULT_MIDI_CHANNEL);
            self.editor_state.data.insert_note(doc_track, note);
            affected.insert(doc_track);
        }
        self.mark_notes_changed();
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
    }

    /// 计算某层应写入的音轨（doc 索引）
    ///
    /// - 显式配置：直接用 `BrushConfig.tracks[level]`。
    /// - 默认：从当前音轨起沿普通音轨（排除 Conductor=0）序行走，
    ///   每个 level 一条不同音轨；不足则循环复用。
    fn brush_track_for_level(&self, level: usize) -> usize {
        if let Some(t) = self.brush.track_for_level(level) {
            return t;
        }
        let num = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(1);
        if num <= 1 {
            return 0; // 仅有指挥轨时兜底
        }
        let normal_count = num - 1; // 排除 conductor(0)
        let cur = self.editor_state.data.current_track;
        let pos_in_normal = if cur == 0 {
            0
        } else {
            (cur - 1).min(normal_count - 1)
        };
        let idx = (pos_in_normal + level) % normal_count;
        1 + idx
    }

    /// 结束画刷笔触（释放时调用）
    pub(crate) fn finish_brush_stroke(&mut self) {
        self.brush_last_cell = None;
        self.editor_state.interaction.clear_preview_sequence();
        self.editor_state.interaction.edit_state = EditState::Idle;
        self.mark_notes_changed();
    }
}
