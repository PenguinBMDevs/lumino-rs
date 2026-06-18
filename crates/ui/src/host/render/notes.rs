//! 洋葱皮数据快照收集 — 在主线程快速快照编辑状态，发送给 NoteWorker
//!
//! == 零拷贝优化 ==
//! `track_notes` 使用 Arc 缓存（`RenderCache::get_or_create_track_notes_arc`），
//! 仅在 `EditorData.track_notes_gen` 变化时重建 Arc，其余帧直接 O(1) clone Arc。
//! `document` 本身已是 `Arc<MidiDocument>`，clone 也是 O(1)。

use std::sync::Arc;

use crate::host::Host;
use lumino_gfx::OnionSkinBucket;

impl Host {
    /// 更新洋葱皮按 key 分桶缓存
    ///
    /// 仅在底层数据变化时重建/增量更新 bucket，避免每帧全量扫描。
    /// 返回值：bucket 是否发生变化（版本号递增）
    pub(super) fn update_onion_bucket(&mut self) -> bool {
        let es = &self.root.editor.editor_state;
        let data = &es.data;
        let current_track = data.current_track;
        let cache = &mut self.render_ctx.render_cache;

        let current_doc_ptr: Option<*const ()> = data
            .document
            .as_ref()
            .map(|arc| Arc::as_ptr(arc) as *const ());
        let current_track_gen = data.track_notes_gen;

        // 情况1：document 变化 → 全量重建
        let doc_changed = cache.onion_bucket_doc_ptr != current_doc_ptr;
        let track_gen_changed = cache.onion_bucket_track_gen != current_track_gen;

        if doc_changed {
            if let Some(doc) = &data.document {
                let new_bucket = OnionSkinBucket::from_midi_document(
                    doc,
                    |_| true, // bucket 包含所有非当前音轨，具体过滤在 collect_visible 时做
                    current_track,
                );
                cache.onion_bucket = Some(new_bucket);
            } else {
                cache.onion_bucket = None;
            }
            cache.onion_bucket_doc_ptr = current_doc_ptr;
            cache.onion_bucket_track_gen = current_track_gen;
            return true;
        }

        // 情况2：只有用户编辑音轨变化 → 增量更新
        if track_gen_changed {
            let bucket = match cache.onion_bucket.as_mut() {
                Some(b) => b,
                None => return false, // 没有 document 时无法建立 bucket，由旧逻辑兜底
            };

            // 移除已不在 track_notes 中的音轨
            let mut tracks_in_bucket: std::collections::HashSet<u16> =
                std::collections::HashSet::new();
            for key in 0..256u16 {
                for note in bucket.key_notes(key as u8) {
                    tracks_in_bucket.insert(note.track_idx());
                }
            }

            let mut changed = false;

            // 移除已不在 track_notes 中的 track
            for track_idx in tracks_in_bucket {
                let track_idx_usize = track_idx as usize;
                if track_idx_usize != current_track
                    && track_idx_usize < 64
                    && !data.track_notes.contains_key(&track_idx_usize)
                {
                    bucket.remove_track(track_idx);
                    changed = true;
                }
            }

            // 更新/添加 track_notes 中的音轨
            for (&track_idx, notes) in &data.track_notes {
                if track_idx == current_track {
                    continue;
                }
                bucket.update_user_track(track_idx as u16, notes.iter());
                changed = true;
            }

            cache.onion_bucket_track_gen = current_track_gen;
            changed
        } else {
            false
        }
    }

    /// 收集洋葱皮计算所需的数据快照
    ///
    /// # 零拷贝设计
    /// - `track_notes` 通过 `RenderCache` 的 Arc 缓存实现，不在本帧全量克隆 HashMap
    /// - `document` 为 `Arc<MidiDocument>`，clone 仅递增引用计数
    /// - 视口参数为 float 标量复制，无内存分配
    pub(super) fn collect_onion_skin_snapshot(
        &mut self,
        viewport_logical_size: (f32, f32),
    ) -> super::note_worker::OnionSkinComputationSnapshot {
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas = &es.canvas;
        let view = &es.view;
        let canvas_width = canvas.size_x;
        let canvas_height = canvas.size_y;
        let keyboard_width = view.keyboard_width;
        let canvas_offset_x = canvas.offset_x;
        let canvas_offset_y = canvas.offset_y;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + canvas_width - keyboard_width) / view.zoom_x).max(visible_tick_start);

        // 从 scroll_y 计算真实的可见 key 范围（与 rendering.rs 保持一致）
        let max_key_index = (view.visible_key_count - 1) as f32;
        let viewport_height = (canvas_height - view.ruler_height).max(0.0);
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);
        let visible_key_max = key_top_f32.ceil() as u16 + 1;
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1);

        // 通过 RenderCache 获取零拷贝 Arc，避免每帧全量克隆 HashMap
        let track_notes_arc = self
            .render_ctx
            .render_cache
            .get_or_create_track_notes_arc(&es.data.track_notes, es.data.track_notes_gen);

        // 更新滚动速度追踪，计算右侧 overscan ticks
        let _velocity = self.scroll_tracker.update(view.scroll_x, view.zoom_x);
        let overscan_ticks = self.scroll_tracker.overscan_ticks(60.0); // 60ms > worker P95 56ms

        super::note_worker::OnionSkinComputationSnapshot {
            // 视口参数（用于瓦片过滤）
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            // 视口参数（用于 NDC 坐标计算，与 note.wgsl 一致）
            scroll_x: view.scroll_x,
            scroll_y: view.scroll_y,
            zoom_x: view.zoom_x,
            zoom_y: view.zoom_y,
            keyboard_width,
            ruler_height: view.ruler_height,
            canvas_offset_x,
            canvas_offset_y,
            viewport_logical_width: viewport_logical_size.0,
            viewport_logical_height: viewport_logical_size.1,
            max_key_index,
            // 洋葱皮数据
            onion_skin_enabled: editor.is_onion_skin_enabled(),
            track_onion_states: self.root.sidebar.get_onion_skin_states(),
            current_track: es.data.current_track,
            document: es.data.document.clone(),
            track_notes: track_notes_arc,
            overscan_ticks,
        }
    }
}
