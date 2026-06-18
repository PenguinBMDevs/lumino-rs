//! 洋葱皮数据快照收集 — 在主线程快速快照编辑状态，发送给 NoteWorker
//!
//! == 零拷贝优化 ==
//! `OnionSkinBucket` 使用 `Arc` 缓存（`RenderCache::update_onion_bucket`），
//! 仅在 `document` 或 `track_notes_gen` 变化时重建/增量更新。
//! 其余帧直接 clone Arc（O(1) 引用计数递增）发给 Worker。

use std::collections::HashSet;
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

        let doc_changed = cache.onion_bucket_doc_ptr != current_doc_ptr;
        let track_gen_changed = cache.onion_bucket_track_gen != current_track_gen;

        if doc_changed {
            let mut bucket = OnionSkinBucket::new();
            if let Some(doc) = &data.document {
                bucket.rebuild_from_midi_document(doc, |_| true, current_track);
            }
            for (&track_idx, notes) in &data.track_notes {
                if track_idx == current_track {
                    continue;
                }
                bucket.update_user_track(track_idx as u16, notes.iter());
            }
            cache.onion_bucket = Some(Arc::new(bucket));
            cache.onion_bucket_doc_ptr = current_doc_ptr;
            cache.onion_bucket_track_gen = current_track_gen;
            return true;
        }

        if track_gen_changed {
            let bucket_arc = match cache.onion_bucket.as_mut() {
                Some(b) => b,
                None => {
                    let mut bucket = OnionSkinBucket::new();
                    for (&track_idx, notes) in &data.track_notes {
                        if track_idx == current_track {
                            continue;
                        }
                        bucket.update_user_track(track_idx as u16, notes.iter());
                    }
                    cache.onion_bucket = Some(Arc::new(bucket));
                    cache.onion_bucket_doc_ptr = current_doc_ptr;
                    cache.onion_bucket_track_gen = current_track_gen;
                    return true;
                }
            };
            let bucket = Arc::make_mut(bucket_arc);

            // 移除已不在 track_notes 中的音轨
            let tracks_in_bucket: HashSet<u16> = (0..256u16)
                .flat_map(|key| bucket.key_notes(key as u8).iter().map(|n| n.track_idx()))
                .collect();
            for track_idx in tracks_in_bucket {
                let track_idx_usize = track_idx as usize;
                if track_idx_usize != current_track
                    && track_idx_usize < 64
                    && !data.track_notes.contains_key(&track_idx_usize)
                {
                    bucket.remove_track(track_idx);
                }
            }

            // 更新/添加 track_notes 中的音轨
            for (&track_idx, notes) in &data.track_notes {
                if track_idx == current_track {
                    continue;
                }
                bucket.update_user_track(track_idx as u16, notes.iter());
            }

            cache.onion_bucket_track_gen = current_track_gen;
            true
        } else {
            false
        }
    }

    /// 收集洋葱皮计算所需的数据快照
    ///
    /// # 零拷贝设计
    /// - `OnionSkinBucket` 通过 RenderCache 的 Arc 缓存，clone 仅递增引用计数
    /// - 视口参数为 float 标量复制，无内存分配
    pub(super) fn collect_onion_skin_snapshot(
        &mut self,
        _viewport_logical_size: (f32, f32),
    ) -> super::note_worker::OnionSkinComputationSnapshot {
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas = &es.canvas;
        let view = &es.view;
        let canvas_width = canvas.size_x;
        let canvas_height = canvas.size_y;
        let keyboard_width = view.keyboard_width;

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

        // 通过 RenderCache 获取零拷贝 Arc，避免每帧全量克隆数据
        let onion_bucket = self
            .render_ctx
            .render_cache
            .onion_bucket
            .as_ref()
            .map(Arc::clone);
        let bucket_version = onion_bucket.as_ref().map_or(0, |b| b.version());

        // 更新滚动速度追踪，计算右侧 overscan ticks
        let _velocity = self.scroll_tracker.update(view.scroll_x, view.zoom_x);
        let overscan_ticks = self.scroll_tracker.overscan_ticks(60.0); // 60ms > worker P95 56ms

        super::note_worker::OnionSkinComputationSnapshot {
            // 视口参数（用于瓦片过滤）
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            // 洋葱皮数据
            onion_skin_enabled: editor.is_onion_skin_enabled(),
            track_onion_states: self.root.sidebar.get_onion_skin_states(),
            current_track: es.data.current_track,
            onion_bucket,
            bucket_version,
            overscan_ticks,
        }
    }
}
