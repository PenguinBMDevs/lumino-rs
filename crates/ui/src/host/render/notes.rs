//! 洋葱皮按 key 分桶缓存维护
//!
//! `OnionSkinBucket` 的增量更新逻辑。
//! 采集已搬到渲染线程（方案 C），本模块只负责维护 bucket 数据。

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
        let _perf = std::time::Instant::now();
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

        let result = if doc_changed {
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
            true
        } else if track_gen_changed {
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
                    return true; // 无现有 bucket，快速重建并返回
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
        };

        // 性能诊断：记录每次 bucket 操作的耗时（超过 500μs 才记，忽略无操作帧）
        let elapsed = _perf.elapsed();
        if result && elapsed.as_micros() > 500 {
            tracing::debug!(
                "update_onion_bucket: changed=true, took={:?} (doc={}, track_gen={})",
                elapsed,
                doc_changed,
                track_gen_changed,
            );
        }
        result
    }
}
