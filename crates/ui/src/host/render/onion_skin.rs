//! 洋葱皮渲染 — MIDI 加载后全量构建所有音轨音符，通过 RenderParams 传到 WGPU 线程
//!
//! 架构（分离渲染线程模式）：
//! - UI 线程：collect_onion_skin_instances 检测 track_notes_gen/mute/current_track 变化
//!   - 变化时重建实例 → RenderParams.onion_skin_instances + onion_skin_dirty=true
//!   - 未变化时 → onion_skin_dirty=false（WGPU 线程复用上一帧 GPU buffer）
//! - WGPU 线程：prepare_renderers 中 dirty=true 时 upload_instances；
//!   execute_render_pass 中每帧 prepare_pass（compute cull）+ draw
//!
//! 性能范式（照搬 wasabi 精神 + lumino 现有基础设施）：
//! - 全量上传一次（MIDI 加载时，非每帧重写）—— 比 wasabi 每帧重写更优
//! - GPU culling 每帧（复用 cull.wgsl 的 workgroup 批量原子 + LOD 剔除）
//! - Indirect draw（CPU 零参与绘制提交）
//! - rayon 并行构建实例（模仿 wasabi mod.rs:130-193）
//!
//! 渲染顺序：洋葱皮（不透明 alpha=1.0）→ 主音轨（不透明）→ UI
//! 深度测试：洋葱皮先绘制 depth=0.0，主音轨后绘制 LessEqual 0.0<=0.0 覆盖

use crate::host::Host;
use lumino_gfx::NoteInstance;

/// 洋葱皮描边宽度（固定 1 像素，与主音轨一致）
const ONION_SKIN_BORDER_WIDTH: u32 = 1;

/// 洋葱皮状态缓存
///
/// 跟踪 track_notes_gen、音轨开关、当前音轨、调色板变化，避免每帧重建全量实例。
/// 字段以值传递（不持有 Host 引用），避免借用冲突。
///
/// `last_current_track` 默认 `usize::MAX`（哨兵值），确保首次 `needs_rebuild`
/// 即使 `current_track == 0` 也会触发重建（`0 != usize::MAX`）。
/// `last_palette_idx` 默认 `u8::MAX`（哨兵值），确保首次或调色板未初始化时触发重建。
pub(crate) struct OnionSkinState {
    /// 上次构建时的 track_notes_gen（用于检测音符数据变化）
    last_track_notes_gen: u64,
    /// 上次构建时的音轨开关指纹（is_muted 位掩码）
    last_mute_fingerprint: u64,
    /// 上次构建时的当前音轨（切换主音轨时需重建）
    last_current_track: usize,
    /// 上次构建时的调色板索引（切换调色板时需重建）
    last_palette_idx: u8,
    /// 是否已初始化（首次上传）
    initialized: bool,
}

impl Default for OnionSkinState {
    fn default() -> Self {
        Self {
            last_track_notes_gen: 0,
            last_mute_fingerprint: 0,
            last_current_track: usize::MAX,
            last_palette_idx: u8::MAX,
            initialized: false,
        }
    }
}

impl OnionSkinState {
    /// 计算音轨开关指纹（is_muted 位掩码）
    ///
    /// 将 sidebar.tracks 的 is_muted 状态压缩为 u64 位掩码（支持 64 轨）
    fn compute_mute_fingerprint(host: &Host) -> u64 {
        let mut fp: u64 = 0;
        for (i, track) in host.root.sidebar.tracks.iter().enumerate() {
            if i >= 64 {
                break;
            }
            if track.is_muted {
                fp |= 1 << i;
            }
        }
        fp
    }

    /// 收集当前重建所需的指纹信息（避免在 needs_rebuild 中持有 Host 借用）
    ///
    /// 返回 `(track_gen, mute_fp, current_track, palette_idx)`：
    /// - `palette_idx` 来自 `lumino_extras::palette::current_palette_idx()`
    pub(super) fn collect_fingerprint(host: &Host) -> (u64, u64, usize, u8) {
        (
            host.root.editor.editor_state.data.track_notes_gen,
            Self::compute_mute_fingerprint(host),
            host.root.editor.editor_state.data.current_track,
            lumino_extras::palette::current_palette_idx(),
        )
    }

    /// 检查是否需要重建洋葱皮实例
    pub(super) fn needs_rebuild(
        &self,
        track_gen: u64,
        mute_fp: u64,
        current_track: usize,
        palette_idx: u8,
    ) -> bool {
        if !self.initialized {
            return true;
        }
        track_gen != self.last_track_notes_gen
            || mute_fp != self.last_mute_fingerprint
            || current_track != self.last_current_track
            || palette_idx != self.last_palette_idx
    }

    /// 标记已构建（在重建后调用）
    pub(super) fn mark_built(
        &mut self,
        track_gen: u64,
        mute_fp: u64,
        current_track: usize,
        palette_idx: u8,
    ) {
        self.last_track_notes_gen = track_gen;
        self.last_mute_fingerprint = mute_fp;
        self.last_current_track = current_track;
        self.last_palette_idx = palette_idx;
        self.initialized = true;
    }
}

impl Host {
    /// 构建洋葱皮全量 NoteInstance 数组
    ///
    /// 遍历所有音轨（除当前主音轨），收集非静音音轨的音符。
    /// 每个音符用音轨颜色（用户设置的调色板 `current_track_color_f32`）打包 key_color。
    /// border_width 固定为 2 像素（用户要求）。
    ///
    /// 性能范式（模仿 wasabi `note_list_system/mod.rs:130-193` 的 rayon 并行写）：
    /// 预收集 `(color, &notes)` 二元组解耦 self 借用，再用 `par_iter().flat_map()`
    /// 并行构建各音轨实例。几十万音符的 MIDI 重建时间可减半。
    fn build_onion_skin_instances(&self) -> Vec<NoteInstance> {
        use rayon::prelude::*;

        let data = &self.root.editor.editor_state.data;
        let current_track = data.current_track;
        let tracks = &self.root.sidebar.tracks;

        // 洋葱皮描边宽度：固定 2 像素（用户要求）
        let border_width = ONION_SKIN_BORDER_WIDTH;

        // 预收集 (color, &notes) 二元组 —— 解耦 self 借用，使 rayon 闭包无需捕获 self
        // 同时完成 mute 过滤，避免并行闭包中查找 tracks
        let track_entries: Vec<([f32; 4], &im::Vector<lumino_note_core::Note>)> = data
            .track_notes
            .iter()
            .filter(|(tid, _)| **tid != current_track)
            .filter_map(|(track_id, notes)| {
                // 音轨开关：静音的音轨不纳入洋葱皮
                let is_muted = tracks
                    .iter()
                    .find(|t| t.id == *track_id)
                    .is_some_and(|t| t.is_muted);
                if is_muted {
                    return None;
                }
                // 音轨颜色：用户设置的调色板（current_track_color_f32）
                let color = lumino_extras::palette::current_track_color_f32(*track_id);
                Some((color, notes))
            })
            .collect();

        // rayon 并行构建：每轨独立并行，轨内顺序构建（im::Vector iter 线程安全）
        let instances: Vec<NoteInstance> = track_entries
            .par_iter()
            .flat_map(|(color, notes)| {
                notes
                    .iter()
                    .map(|note| {
                        NoteInstance::new(
                            note.tick,
                            note.key as u8,
                            note.length,
                            *color,
                            border_width,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        tracing::debug!(
            "Built {} onion skin instances from {} tracks (excluding track {})",
            instances.len(),
            data.track_notes.len().saturating_sub(1),
            current_track
        );
        instances
    }

    /// 收集洋葱皮实例（分离渲染线程模式，由 collect_render_data 调用）
    ///
    /// 返回 `(instances, dirty)`：
    /// - `dirty=true`：实例已重建，WGPU 线程需重上传 GPU buffer
    /// - `dirty=false`：实例未变化，WGPU 线程复用上一帧的 GPU buffer（instances 为空）
    pub(super) fn collect_onion_skin_instances(&mut self) -> (Vec<NoteInstance>, bool) {
        // 走带模式跳过
        if self.root.is_arrangement_mode() {
            return (Vec::new(), false);
        }

        let (track_gen, mute_fp, current_track, palette_idx) =
            OnionSkinState::collect_fingerprint(self);
        let needs_rebuild = self.render_ctx.onion_skin_state.needs_rebuild(
            track_gen,
            mute_fp,
            current_track,
            palette_idx,
        );

        if needs_rebuild {
            let instances = self.build_onion_skin_instances();
            self.render_ctx.onion_skin_state.mark_built(
                track_gen,
                mute_fp,
                current_track,
                palette_idx,
            );
            tracing::debug!(
                "[onion-skin] 重建 {} 个实例 (track_gen={}, current_track={}, palette_idx={})",
                instances.len(),
                track_gen,
                current_track,
                palette_idx
            );
            (instances, true)
        } else {
            (Vec::new(), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onion_skin_state_default_uninitialized() {
        let state = OnionSkinState::default();
        assert!(!state.initialized);
        assert_eq!(state.last_track_notes_gen, 0);
        assert_eq!(state.last_mute_fingerprint, 0);
        assert_eq!(state.last_current_track, usize::MAX);
        assert_eq!(state.last_palette_idx, u8::MAX);
    }

    #[test]
    fn onion_skin_state_needs_rebuild_on_first_run() {
        let state = OnionSkinState::default();
        assert!(state.needs_rebuild(0, 0, 0, 0));
    }

    #[test]
    fn onion_skin_state_no_rebuild_after_mark_built() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0b1010, 3, 1);
        assert!(!state.needs_rebuild(42, 0b1010, 3, 1));
    }

    #[test]
    fn onion_skin_state_rebuild_on_gen_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0, 0, 0);
        assert!(state.needs_rebuild(43, 0, 0, 0));
    }

    #[test]
    fn onion_skin_state_rebuild_on_mute_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0b0000, 0, 0);
        assert!(state.needs_rebuild(42, 0b0001, 0, 0));
    }

    #[test]
    fn onion_skin_state_rebuild_on_track_switch() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0, 1, 0);
        assert!(state.needs_rebuild(42, 0, 2, 0));
    }

    #[test]
    fn onion_skin_state_rebuild_on_palette_switch() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0, 0, 1);
        assert!(state.needs_rebuild(42, 0, 0, 2));
    }
}
