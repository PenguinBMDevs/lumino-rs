//! 洋葱皮音符列表维护 — 参考 Wasabi 瀑布流实现的简化版本
//!
//! `OnionNoteList` 的增量更新逻辑。
//! 替换了旧版 `OnionSkinBucket` 的 per-key 分桶缓存。
//! 现在使用扁平 `Vec<OnionNote>` 存储所有洋葱皮音符。

use std::collections::HashSet;
use std::sync::Arc;

use crate::host::Host;
use lumino_gfx::OnionNoteList;

impl Host {
    /// 更新洋葱皮音符列表
    ///
    /// 仅在底层数据变化时重建/增量更新 note list。
    /// 返回值：note list 是否发生变化（版本号递增）
    pub(super) fn update_onion_note_list(&mut self) -> bool {
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

        let doc_changed = cache.onion_list_doc_ptr != current_doc_ptr;
        let track_gen_changed = cache.onion_list_track_gen != current_track_gen;

        let result = if doc_changed {
            let mut list = OnionNoteList::new();
            if let Some(doc) = &data.document {
                list.rebuild_from_midi_document(doc, |_| true, current_track);
            }
            for (&track_idx, notes) in &data.track_notes {
                if track_idx == current_track {
                    continue;
                }
                list.update_user_track(track_idx as u16, notes.iter());
            }
            cache.onion_note_list = Some(Arc::new(list));
            cache.onion_list_doc_ptr = current_doc_ptr;
            cache.onion_list_track_gen = current_track_gen;
            true
        } else if track_gen_changed {
            let list_arc = match cache.onion_note_list.as_mut() {
                Some(l) => l,
                None => {
                    let mut list = OnionNoteList::new();
                    for (&track_idx, notes) in &data.track_notes {
                        if track_idx == current_track {
                            continue;
                        }
                        list.update_user_track(track_idx as u16, notes.iter());
                    }
                    cache.onion_note_list = Some(Arc::new(list));
                    cache.onion_list_doc_ptr = current_doc_ptr;
                    cache.onion_list_track_gen = current_track_gen;
                    return true;
                }
            };
            let list = Arc::make_mut(list_arc);

            // 移除已不在 track_notes 中的音轨
            let tracks_in_list: HashSet<u16> =
                list.as_slice().iter().map(|n| n.track_idx()).collect();
            for track_idx in tracks_in_list {
                let track_idx_usize = track_idx as usize;
                if track_idx_usize != current_track
                    && track_idx_usize < 64
                    && !data.track_notes.contains_key(&track_idx_usize)
                {
                    list.remove_track(track_idx);
                }
            }

            // 更新/添加 track_notes 中的音轨
            for (&track_idx, notes) in &data.track_notes {
                if track_idx == current_track {
                    continue;
                }
                list.update_user_track(track_idx as u16, notes.iter());
            }

            cache.onion_list_track_gen = current_track_gen;
            true
        } else {
            false
        };

        // 性能诊断：记录每次操作的耗时（超过 500μs 才记）
        let elapsed = _perf.elapsed();
        if result && elapsed.as_micros() > 500 {
            tracing::debug!(
                "update_onion_note_list: changed=true, took={:?} (doc={}, track_gen={})",
                elapsed,
                doc_changed,
                track_gen_changed,
            );
        }
        result
    }
}
