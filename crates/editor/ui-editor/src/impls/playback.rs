//! 播放键色更新 —— 编辑器侧适配层
//!
//! 算法本体在 [`lumino_note_core::playback_key_colors`]（编辑器实时播放与视频
//! 导出**共用唯一实现**，历史上是两份已漂移的拷贝）。本模块只负责两件事：
//!
//! 1. 把编辑器状态映射到算法入参（播放位置 → tick、`track_notes_gen` → 内容世代）；
//! 2. **选定取色策略**：与卷帘音符同源（`lumino_extras::palette::note_track_color`），
//!    即「主音轨 = 固定蓝、其余轨 = 调色板色」。
//!
//! 拆分历史：`editor_impl.rs` 接近 400 行限制时按职责拆出本文件。

use crate::Editor;
use lumino_extras::palette::note_track_color;

/// 播放键色扫描状态（算法本体在 `lumino_note_core`）
pub(crate) type PlaybackScanState = lumino_note_core::PlaybackKeyColorState;

impl Editor {
    /// 根据当前播放位置，计算每个 key 上被音符覆盖的颜色
    ///
    /// 播放停止时（`playback_position == 0.0`）清空颜色立即返回。
    /// 当 `playback_key_colors_enabled == false` 时直接返回。
    ///
    /// # 取色单源
    ///
    /// 与卷帘音符实例完全同源（`lumino_extras::palette::note_track_color`）：
    /// 主音轨（当前编辑轨）= `MAIN_TRACK_NOTE_COLOR` 固定蓝，其余轨 = 调色板色。
    /// 2026-10-25 之前键盘对所有轨一律用调色板色，主音轨的键色与音符色因此不一致。
    ///
    /// # 性能
    ///
    /// 每帧 O(本帧新开始音符数 + 音轨数 × log + 256)；回退 O(快照间隔内音符)。
    /// **与「同时发声音符数」无关**（算法本体文档见 `lumino_note_core`）。
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        // 停止播放（位置为 0）：清空颜色并归零聚合。
        // 快照**保留**——同一文档再次播放 / 回绕时可继续复用。
        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            self.playback_scan_state.reset_aggregate();
            return;
        }

        let tick = self.playback_position as u32;
        let data = &self.editor_state.data;
        let Some(doc) = data.document.as_ref() else {
            return;
        };
        let content_gen = data.track_notes_gen;
        let current_track = data.current_track;
        // 键盘键色 = 卷帘音符色（键位所在音轨 → 该轨音符颜色）
        let color_of_track = move |track: usize| note_track_color(track, current_track);

        self.playback_scan_state.update(
            doc,
            tick,
            content_gen,
            &color_of_track,
            &mut self.playback_key_colors,
        );
    }

    /// 推进后台键色快照预热（每帧调用，最多消费 `note_budget` 个音符）
    ///
    /// 用于把「装载后首次 seek」的 O(全曲音符) 重扫摊到多帧。返回 `true`
    /// 表示仍需继续。**应在每帧（而非仅播放时）驱动**，否则装载后直接拖进度条
    /// 仍会撞上一次性冻结。
    pub fn prewarm_playback_key_colors(&mut self, note_budget: usize) -> bool {
        if !self.playback_key_colors_enabled {
            return false;
        }
        let data = &self.editor_state.data;
        let Some(doc) = data.document.as_ref() else {
            return false;
        };
        self.playback_scan_state.prewarm_step(doc, note_budget)
    }

    /// 清空播放键色并重置增量扫描状态
    ///
    /// 停止播放（手动停止或自然结束）后调用，使键盘恢复为「无颜色」状态。
    ///
    /// 仅重置颜色数组与聚合，**不改变** `playback_position`：
    /// 自然结束时播放指示线应停留在结束位置（而非跳回 0），
    /// 手动停止由调用方负责复位 `playback_position`。
    ///
    /// 快照保留：同一文档的再次播放 / 循环回绕可继续复用，避免重付 O(全曲) 重扫。
    ///
    /// 当 `playback_key_colors_enabled == false` 时颜色数组本就是全零，调用安全无损。
    pub fn clear_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if self.playback_key_colors != [0u8; 1024] {
            self.playback_key_colors = [0u8; 1024];
        }
        self.playback_scan_state.reset_aggregate();
    }
}
