//! Root 工程走带（音轨总览）视图方法
//!
//! 最大 tick 缓存（按 track_notes_gen 失效）、自动滚动、播放状态查询。

use crate::root::Root;

impl Root {
    /// 获取工程走带视图的最大 tick 终点（缓存，按 track_notes_gen 失效）
    ///
    /// 播放时每帧需要计算最大滚动范围，全量扫描音符在大型 MIDI 下会导致主线程卡顿。
    /// 2026-08-06 性能修复：改由 `MidiDocument::tracks_max_end_tick()` 提供每轨增量缓存
    /// （插入 O(1) 更新、删除置脏惰性重算），1600W 工程首帧全量扫描 29.8ms → O(音轨数)。
    /// 保留 track_notes_gen 缓存作为二次保险（跨 document 替换时避免重复查询）。
    pub fn arrangement_max_tick_end(&mut self) -> f32 {
        let editor_data = &self.editor.editor_state.data;
        let vp = &mut self.arrangement_view.viewport;
        let current_gen = editor_data.track_notes_gen;
        if vp.cached_track_notes_gen != current_gen {
            vp.cached_max_tick_end = editor_data
                .document
                .as_ref()
                .map(|doc| doc.tracks_max_end_tick() as f32)
                .unwrap_or(0.0);
            vp.cached_track_notes_gen = current_gen;
        }
        vp.cached_max_tick_end
            .max(crate::constants::editor::DEFAULT_MIN_TICKS)
    }

    /// 更新工程走带视图的自动滚动（基于编辑器自动滚动配置）
    /// 使演奏指示线的滚动模式在工程走带界面同样适用
    pub fn update_arrangement_auto_scroll(&mut self, playback_tick: f32) {
        let asc = *self.editor.auto_scroll_config();
        if asc.mode == lumino_core::storage::config::AutoScrollMode::Off {
            return;
        }

        // 先计算缓存的最大 tick（可能扫描 track_notes），再借用 viewport
        let max_tick = self.arrangement_max_tick_end();

        let vp = &mut self.arrangement_view.viewport;
        let viewport_width = vp.canvas_size.x.max(1.0);
        let ppu = vp.zoom_x.max(0.001);

        // 计算最大滚动值（使用视口尺寸和总宽度）
        let canvas_w = vp.canvas_size.x.max(1.0);
        let total_w = max_tick * vp.zoom_x;
        let max_scroll = (total_w - canvas_w).max(0.0);

        match asc.mode {
            lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;
                let target_scroll_x = playback_tick * ppu - indicator_pos;
                // 到达末尾时自动松开固定，滚动停在末尾
                vp.scroll_x = target_scroll_x.clamp(0.0, max_scroll);
            }
            lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => {
                let trigger_offset = asc.page_trigger_offset as f32;
                let return_pos = asc.page_return_position as f32;
                let indicator_screen_x = playback_tick * ppu - vp.scroll_x;

                if indicator_screen_x >= viewport_width - trigger_offset {
                    let target_scroll_x = playback_tick * ppu - return_pos;
                    vp.scroll_x = target_scroll_x.clamp(0.0, max_scroll);
                }
            }
            lumino_core::storage::config::AutoScrollMode::Off => {}
        }
    }

    /// 获取播放状态（通过 `last_frame` 缓存非阻塞读取，避免锁争用）
    pub fn is_playing(&self) -> bool {
        self.playback
            .manager
            .as_ref()
            .map(|m| {
                m.last_frame()
                    .is_some_and(|f| f.state == crate::playback::PlaybackState::Playing)
            })
            .unwrap_or_default()
    }
}
