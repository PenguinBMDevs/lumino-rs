//! 纯水平滚动增量判定（`state.rs` 子模块）。

use super::{CpuOnionFrameKey, CpuOnionState};

impl CpuOnionState {
    /// 纯水平滚动的整数物理像素位移（不满足条件返回 `None`）。
    ///
    /// 条件全部满足才允许增量：除 `scroll_x` 外整份布局键一致（尺寸/原点/
    /// 纵向滚动/缩放/键数/当前轨/静音/调色板/代次），位移是整数物理像素且
    /// 不超过帧宽 60%。任一不满足都退回整帧，绝不拿旧帧猜新相机。
    pub(super) fn scroll_dx(&self, key: &CpuOnionFrameKey) -> Option<i32> {
        let frozen = self.frozen.as_ref()?;
        if frozen.track_notes_gen != key.track_notes_gen
            || frozen.current_track != key.current_track
            || frozen.mute_fingerprint != key.mute_fingerprint
            || frozen.palette_idx != key.palette_idx
        {
            return None;
        }
        let previous = frozen.geometry;
        let next = key.geometry;
        if previous.x != next.x
            || previous.y != next.y
            || previous.width != next.width
            || previous.height != next.height
        {
            return None;
        }
        let old = previous.camera;
        let new = next.camera;
        if old.scroll_y != new.scroll_y
            || old.zoom_x != new.zoom_x
            || old.zoom_y != new.zoom_y
            || old.key_count != new.key_count
        {
            return None;
        }
        let delta = new.scroll_x - old.scroll_x;
        if !delta.is_finite() {
            return None;
        }
        let rounded = delta.round();
        if rounded == 0.0 || (delta - rounded).abs() > 0.01 {
            return None;
        }
        let dx = rounded as i32;
        let width = i64::from(next.width);
        let distance = i64::from(dx).unsigned_abs();
        if distance >= width.unsigned_abs() || distance > (width * 3 / 5).max(1) as u64 {
            return None;
        }
        Some(dx)
    }
}
