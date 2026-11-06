//! 洋葱皮显示范围解析 —— 「模式 + 当前轨 + 轨数」→ GPU uniform 的 track_enc 闭区间
//!
//! # 为什么把模式解析成区间而不是把模式本身下发
//!
//! shader 只需要回答一个问题：「这个实例的 track_enc 在不在显示范围内」。
//! 区间（两个字）足以表达全部四种模式，不必让 WGSL 懂业务模式：
//! - 全部音轨 → `(1, u32::MAX)`（所有编码）
//! - 上一个音轨 → `(cur-1, cur-1)`（首轨时为空）
//! - 下一个音轨 → `(cur+1, cur+1)`（末轨时为空）
//! - 指定音轨 → 本轮待接入，**空区间**（宁可不显示，也不误显示别的轨）
//!
//! 空区间用 `first > last` 表示（这里取 `(1, 0)`），shader 侧的条件比较天然不命中，
//! 不需要额外的"有效位"。
//!
//! # 适用范围（重要）
//!
//! 本区间只作用于 **GPU 实例层**（`SetViewState` → `onion_note*.wgsl` / `cull*.wgsl`）。
//! CPU 后端（`OnionSkinBackend::Cpu`）的洋葱皮由 CPU 光栅器统一合成，不消费本区间——
//! 该后端在设置页已明确标注为降级方案（不支持纵向卷帘 / 走带 / 瀑布流 / 视频导出）。

use lumino_ui_core::onion_skin_mode::OnionSkinMode;

/// 空区间哨兵：`first > last` ⇒ shader 侧 `enc >= first && enc <= last` 恒假
pub(crate) const EMPTY_ONION_RANGE: (u32, u32) = (1, 0);

/// 解析洋葱皮显示范围（track_enc 闭区间）
///
/// - `current_track`：当前主音轨索引（0 基，来自文档）
/// - `track_count`：文档音轨总数（判定"下一个"是否存在）
/// - `enabled`：工具栏总开关（关闭时直接给空区间：省一次 shader 判断，
///   也保证"关掉后不会因为模式残留而漏出幽灵音符"）
pub(crate) fn onion_track_range(
    mode: OnionSkinMode,
    current_track: usize,
    track_count: usize,
    enabled: bool,
) -> (u32, u32) {
    if !enabled {
        return EMPTY_ONION_RANGE;
    }
    // track_enc = track_idx + 1（0 是"无主音轨"哨兵，永远不参与范围）
    let current_enc = current_track as u32 + 1;
    match mode {
        OnionSkinMode::AllTracks => (1, u32::MAX),
        OnionSkinMode::PreviousTrack => {
            if current_enc > 1 {
                (current_enc - 1, current_enc - 1)
            } else {
                EMPTY_ONION_RANGE
            }
        }
        OnionSkinMode::NextTrack => {
            // 合法 enc 上界 = track_count（enc = idx+1），末轨没有"下一个"
            if current_enc < track_count as u32 {
                (current_enc + 1, current_enc + 1)
            } else {
                EMPTY_ONION_RANGE
            }
        }
        // 指定音轨：本轮只留入口（`is_implemented() == false`），不放行任何轨
        OnionSkinMode::SpecificTrack => EMPTY_ONION_RANGE,
    }
}

#[cfg(test)]
mod tests {
    use super::{EMPTY_ONION_RANGE, onion_track_range};
    use lumino_ui_core::onion_skin_mode::OnionSkinMode;

    /// 全部音轨 = 所有编码（不依赖轨数：超大编制工程也不截断）
    #[test]
    fn test_all_tracks_covers_every_encoding() {
        assert_eq!(
            onion_track_range(OnionSkinMode::AllTracks, 0, 3, true),
            (1, u32::MAX)
        );
        assert_eq!(
            onion_track_range(OnionSkinMode::AllTracks, 999, 1, true),
            (1, u32::MAX)
        );
    }

    /// 上一个音轨：正常情况只放行 cur-1；首轨无上一个 → 空区间
    #[test]
    fn test_previous_track_range() {
        // 当前轨 idx 2（enc 3）→ 只显示 idx 1（enc 2）
        assert_eq!(
            onion_track_range(OnionSkinMode::PreviousTrack, 2, 5, true),
            (2, 2)
        );
        // 首轨（idx 0，enc 1）没有上一个
        assert_eq!(
            onion_track_range(OnionSkinMode::PreviousTrack, 0, 5, true),
            EMPTY_ONION_RANGE
        );
    }

    /// 下一个音轨：正常情况只放行 cur+1；末轨无下一个 → 空区间
    #[test]
    fn test_next_track_range() {
        assert_eq!(
            onion_track_range(OnionSkinMode::NextTrack, 1, 5, true),
            (3, 3)
        );
        // 末轨（idx 4，enc 5，轨数 5）没有下一个
        assert_eq!(
            onion_track_range(OnionSkinMode::NextTrack, 4, 5, true),
            EMPTY_ONION_RANGE
        );
    }

    /// 开关关闭 → 空区间（模式保留但不放行任何轨）
    #[test]
    fn test_disabled_yields_empty_range() {
        for mode in OnionSkinMode::all() {
            assert_eq!(
                onion_track_range(mode, 2, 5, false),
                EMPTY_ONION_RANGE,
                "关闭开关时 {mode:?} 必须为空区间"
            );
        }
    }

    /// 指定音轨未接入：必须空区间（宁可什么都不显示，也不误显示别的轨）
    #[test]
    fn test_specific_track_is_empty_until_implemented() {
        assert_eq!(
            onion_track_range(OnionSkinMode::SpecificTrack, 2, 5, true),
            EMPTY_ONION_RANGE
        );
    }

    /// 单轨工程：上一个/下一个都为空，全部 = 全部（但只有主音轨，故无洋葱皮）
    #[test]
    fn test_single_track_project_has_no_onion_side() {
        assert_eq!(
            onion_track_range(OnionSkinMode::PreviousTrack, 0, 1, true),
            EMPTY_ONION_RANGE
        );
        assert_eq!(
            onion_track_range(OnionSkinMode::NextTrack, 0, 1, true),
            EMPTY_ONION_RANGE
        );
    }
}
