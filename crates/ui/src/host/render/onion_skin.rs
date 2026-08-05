//! 洋葱皮渲染 — MIDI 加载后全量上传所有音轨音符，GPU culling + indirect draw
//!
//! 性能范式（照搬 wasabi 精神 + lumino 现有基础设施）：
//! - 全量上传一次（MIDI 加载时，非每帧重写）—— 比 wasabi 每帧重写更优
//! - GPU culling 每帧（复用 cull.wgsl 的 workgroup 批量原子 + LOD 剔除）
//! - Indirect draw（CPU 零参与绘制提交）
//! - 音轨开关变化时重建（O(N_notes) 但不频繁）
//!
//! 渲染顺序：洋葱皮（半透明 alpha=0.3）→ 主音轨（不透明）→ UI

use crate::host::Host;
use lumino_gfx::{NoteInstance, calculate_border_width};

/// 洋葱皮状态缓存
///
/// 跟踪 track_notes_gen 和音轨开关变化，避免每帧重建全量实例。
/// 字段以值传递（不持有 Host 引用），避免借用冲突。
///
/// `last_current_track` 默认 `usize::MAX`（哨兵值），确保首次 `needs_rebuild`
/// 即使 `current_track == 0` 也会触发重建（`0 != usize::MAX`）。
pub(crate) struct OnionSkinState {
    /// 上次构建时的 track_notes_gen（用于检测音符数据变化）
    last_track_notes_gen: u64,
    /// 上次构建时的音轨开关指纹（is_muted 位掩码）
    last_mute_fingerprint: u64,
    /// 上次构建时的当前音轨（切换主音轨时需重建）
    last_current_track: usize,
    /// 是否已初始化（首次上传）
    initialized: bool,
}

impl Default for OnionSkinState {
    fn default() -> Self {
        Self {
            last_track_notes_gen: 0,
            last_mute_fingerprint: 0,
            last_current_track: usize::MAX,
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
    pub(super) fn collect_fingerprint(host: &Host) -> (u64, u64, usize) {
        (
            host.root.editor.editor_state.data.track_notes_gen,
            Self::compute_mute_fingerprint(host),
            host.root.editor.editor_state.data.current_track,
        )
    }

    /// 检查是否需要重建洋葱皮实例
    pub(super) fn needs_rebuild(&self, track_gen: u64, mute_fp: u64, current_track: usize) -> bool {
        if !self.initialized {
            return true;
        }
        track_gen != self.last_track_notes_gen
            || mute_fp != self.last_mute_fingerprint
            || current_track != self.last_current_track
    }

    /// 标记已构建（在 upload 后调用）
    fn mark_built(&mut self, track_gen: u64, mute_fp: u64, current_track: usize) {
        self.last_track_notes_gen = track_gen;
        self.last_mute_fingerprint = mute_fp;
        self.last_current_track = current_track;
        self.initialized = true;
    }
}

impl Host {
    /// 构建洋葱皮全量 NoteInstance 数组
    ///
    /// 遍历所有音轨（除当前主音轨），收集非静音音轨的音符。
    /// 每个音符用音轨颜色（ARRANGEMENT_PALETTE）打包 key_color。
    /// border_width 与主音轨一致（wasabi 风格全局共享值）。
    ///
    /// 性能范式（模仿 wasabi `note_list_system/mod.rs:130-193` 的 rayon 并行写）：
    /// 预收集 `(color, &notes)` 二元组解耦 self 借用，再用 `par_iter().flat_map()`
    /// 并行构建各音轨实例。几十万音符的 MIDI 重建时间可减半。
    fn build_onion_skin_instances(&self) -> Vec<NoteInstance> {
        use rayon::prelude::*;

        let data = &self.root.editor.editor_state.data;
        let current_track = data.current_track;
        let tracks = &self.root.sidebar.tracks;

        // 计算 border_width（与主音轨一致）
        let view = &self.root.editor.editor_state.view;
        let canvas = &self.root.editor.editor_state.canvas;
        let key_axis_pixels = (canvas.size_y - view.ruler_height).max(1.0);
        let border_width = calculate_border_width(key_axis_pixels, view.visible_key_count as f32);

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
                // 音轨颜色：ARRANGEMENT_PALETTE 循环取色
                let palette_idx = track_id % lumino_gfx::ARRANGEMENT_PALETTE.len();
                let rgb = lumino_gfx::ARRANGEMENT_PALETTE[palette_idx];
                let color = [rgb[0], rgb[1], rgb[2], 1.0];
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

    /// 准备洋葱皮渲染（全量上传 + GPU cull）
    ///
    /// 调用时机：每帧（内部自动判断是否需要重建）
    ///
    /// 性能：仅在 needs_rebuild 为 true 时重建，否则只跑 compute cull
    pub(super) fn prepare_onion_skin(
        &mut self,
        gfx: &lumino_gfx::Context,
        encoder: &mut iced_wgpu::wgpu::CommandEncoder,
        camera: lumino_gfx::CameraUniform,
    ) {
        puffin::profile_function!();

        // 走带模式或无 renderer 时跳过
        if self.root.is_arrangement_mode() {
            return;
        }

        // 收集指纹（不可变借用，提前释放）
        let (track_gen, mute_fp, current_track) = OnionSkinState::collect_fingerprint(self);

        // 检查是否需要重建（可变借用 render_ctx）
        let needs_rebuild =
            self.render_ctx
                .onion_skin_state
                .needs_rebuild(track_gen, mute_fp, current_track);

        if needs_rebuild {
            // 构建实例（不可变借用 root）
            let instances = self.build_onion_skin_instances();
            // 上传到 GPU（可变借用 render_ctx）
            if let Some(onion_renderer) = self.render_ctx.onion_skin_renderer.as_mut() {
                onion_renderer.upload_instances(&instances, &gfx.device, &gfx.queue);
                tracing::debug!("Onion skin rebuilt: {} instances uploaded", instances.len());
            }
            // 标记已构建（可变借用 render_ctx）
            self.render_ctx
                .onion_skin_state
                .mark_built(track_gen, mute_fp, current_track);
        }

        // 每帧跑 compute cull（视口变化时重新裁剪）
        if let Some(onion_renderer) = self.render_ctx.onion_skin_renderer.as_mut() {
            onion_renderer.prepare_pass(encoder, camera, &gfx.queue);
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
    }

    #[test]
    fn onion_skin_state_needs_rebuild_on_first_run() {
        let state = OnionSkinState::default();
        assert!(state.needs_rebuild(0, 0, 0));
    }

    #[test]
    fn onion_skin_state_no_rebuild_after_mark_built() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0b1010, 3);
        assert!(!state.needs_rebuild(42, 0b1010, 3));
    }

    #[test]
    fn onion_skin_state_rebuild_on_gen_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0, 0);
        assert!(state.needs_rebuild(43, 0, 0));
    }

    #[test]
    fn onion_skin_state_rebuild_on_mute_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0b0000, 0);
        assert!(state.needs_rebuild(42, 0b0001, 0));
    }

    #[test]
    fn onion_skin_state_rebuild_on_track_switch() {
        let mut state = OnionSkinState::default();
        state.mark_built(42, 0, 1);
        assert!(state.needs_rebuild(42, 0, 2));
    }
}
