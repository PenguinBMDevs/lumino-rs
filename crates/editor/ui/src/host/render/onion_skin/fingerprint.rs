//! 洋葱皮指纹 — 每帧比较的输入快照（音符代次 / 静音集合 / 当前轨 / 调色板 / 轨数）。

use crate::host::Host;
use crate::host::render::onion_skin::OnionSkinState;

/// 洋葱皮指纹信息（用于 needs_rebuild 比较）
pub(crate) struct OnionSkinFingerprint {
    /// 文档音符代次（`track_notes_gen`）
    pub(super) track_gen: u64,
    /// 静音集合指纹（顺序无关的 id 哈希）
    pub(super) mute_fp: u64,
    /// 当前音轨（主音轨，洋葱皮不显示）
    pub(super) current_track: usize,
    /// 当前调色板索引（实例颜色固化，切换需重传）
    pub(super) palette_idx: u8,
    /// 本次 `track_gen` 变化明确影响的音轨集合（`None` = 未知→保守全量重建）
    ///
    /// 增量优化（2026-08-05）：编辑操作（如 `mark_current_track_changed`）记录
    /// 精确影响的音轨。洋葱皮不显示当前音轨与静音音轨——当变化音轨全部落在
    /// 该集合内时，洋葱皮 GPU 数据实际未变，可豁免每帧全量重建上传。
    pub(super) onion_dirty_tracks: Option<std::collections::HashSet<usize>>,
    /// 当前静音音轨集合（洋葱皮跳过，参与豁免判断）
    pub(super) muted_tracks: Vec<usize>,
    /// 当前轨结构性变化（批量插入/删除、undo 整轨替换）→ 需要主轨段重建
    ///
    /// 无段内增量事件可对账的大变化：以单轨 `TrackDelta` 重建当前轨段，
    /// 替代全量会话重建（大工程下 CPU/GPU 成本差 ~5x）。
    pub(super) main_track_struct_dirty: bool,
    /// 文档当前音轨总数（段表长度基准；变化 → `TrackLayout` 增量同步）
    pub(super) track_count: usize,
}

impl OnionSkinState {
    /// 计算音轨开关指纹（is_muted 状态 hash）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用 / 不得限制音轨数量。
    /// 旧实现用 u64 位掩码仅支持 64 轨（超出 break 截断），
    /// 新实现用 hash 支持任意音轨数量。
    ///
    /// 顺序无关：仅对「静音音轨 id 集合」哈希（集合先排序再哈希）。
    /// 音轨拖拽排序只改变 sidebar.tracks 顺序、不改变静音集合与
    /// document 绑定，排序不应触发洋葱皮全量重建（洋葱皮显示与
    /// sidebar 顺序无关，按 track_id 组织段表）。
    pub(super) fn compute_mute_fingerprint(host: &Host) -> u64 {
        let mut muted_ids: Vec<usize> = host
            .root
            .sidebar
            .tracks
            .iter()
            .filter(|t| t.is_muted)
            .map(|t| t.id)
            .collect();
        mute_fingerprint_of(&mut muted_ids)
    }

    /// 收集当前重建所需的指纹信息（避免在 needs_rebuild 中持有 Host 借用）
    pub(crate) fn collect_fingerprint(host: &Host) -> OnionSkinFingerprint {
        let data = &host.root.editor.editor_state.data;
        let muted_tracks = host
            .root
            .sidebar
            .tracks
            .iter()
            .filter(|t| t.is_muted)
            .map(|t| t.id)
            .collect();
        OnionSkinFingerprint {
            track_gen: data.track_notes_gen,
            mute_fp: Self::compute_mute_fingerprint(host),
            current_track: data.current_track,
            palette_idx: lumino_extras::palette::current_palette_idx(),
            onion_dirty_tracks: data.onion_dirty_tracks.clone(),
            muted_tracks,
            main_track_struct_dirty: data.main_track_struct_dirty,
            track_count: data
                .document
                .as_ref()
                .map(|doc| doc.track_count())
                .unwrap_or(0),
        }
    }
}

/// 计算静音音轨集合的指纹（顺序无关）
///
/// 排序后哈希：同一集合无论侧边栏排列顺序如何，指纹一致。
/// 音轨拖拽排序因此不会触发洋葱皮全量重建。
pub(crate) fn mute_fingerprint_of(muted_ids: &mut [usize]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    muted_ids.sort_unstable();
    let mut hasher = DefaultHasher::new();
    for id in muted_ids.iter() {
        id.hash(&mut hasher);
    }
    hasher.finish()
}
