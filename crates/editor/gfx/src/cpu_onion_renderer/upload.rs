//! 上传决策状态机（纯逻辑，不依赖 GPU，便于单元测试）。
//!
//! Step 5b 起支持**脏矩形局部上传**：帧带合法脏区且尺寸未变时只重写该子矩形，
//! 其余像素保留上一版；尺寸变化 / 状态缺失 / 脏区不可信时保守回退全量。
//!
//! # 判定顺序（**固定**，不得交换）
//!
//! 1. 帧无效（尺寸为 0 / 像素长度不匹配）→ `Skip`；
//! 2. 无驻留纹理、或尺寸变化 → `RecreateAndUpload`（**忽略脏区**：新纹理必须铺满）；
//! 3. 尺寸相同且版本相同 → `Reuse`（内容同一份，脏区无从谈起）；
//! 4. 尺寸相同、版本变化且脏区合法、**版本紧邻**（`resident + 1 == frame`）→
//!    `PartialUpload(rect)`；
//! 5. 其余（版本变化但无脏区 / 脏区为空或越界 / **版本跳号**）→ `Upload` 全量。
//!
//! 顺序即正确性：先把「不能只写一块」的情形全部挡掉，再谈局部上传。
//!
//! **版本紧邻是局部上传的隐含前提**：渲染侧三缓冲会合并掉中间帧，版本跳号说明
//! 中间那次区域重绘从未落进纹理——此时只写本帧脏区会永久留下旧像素，必须全量。

use super::{CpuOnionUploadRect, frame::CpuOnionFrame};

/// 一帧对应的纹理上传动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UploadPlan {
    /// 帧无效（尺寸为 0 / 像素长度不匹配）：不绘制、不上传
    Skip,
    /// 驻留纹理尺寸与帧尺寸不同：重建纹理并全量上传
    RecreateAndUpload,
    /// 尺寸相同但内容版本变化：全量上传（复用纹理）
    Upload,
    /// 尺寸相同、版本变化且脏区完整落在帧内：只重写该子矩形
    PartialUpload(CpuOnionUploadRect),
    /// 尺寸与版本都相同：跳过上传，直接复用驻留纹理
    Reuse,
}

/// 上传状态：记录驻留纹理的尺寸与内容版本。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UploadState {
    size: Option<(u32, u32)>,
    version: Option<u64>,
}

impl UploadState {
    /// 计算本帧需要执行的上传动作（判定顺序见模块文档，固定不可换）。
    pub(super) fn plan(&self, frame: &CpuOnionFrame) -> UploadPlan {
        if !frame.validate() {
            return UploadPlan::Skip;
        }
        match self.size {
            // 尚无驻留纹理
            None => UploadPlan::RecreateAndUpload,
            // 尺寸变化：旧纹理无法承载新帧（脏区无意义，必须整帧铺满新纹理）
            Some(size) if size != frame.size() => UploadPlan::RecreateAndUpload,
            // 尺寸相同、版本相同：内容同一份，任何脏区标注都不该触发上传
            Some(_) if self.version == Some(frame.version) => UploadPlan::Reuse,
            // 尺寸相同、版本变化：脏区可信且版本紧邻才局部写，否则全量
            Some(_) => {
                let contiguous = self
                    .version
                    .is_some_and(|version| version.wrapping_add(1) == frame.version);
                let dirty = frame
                    .dirty_rect
                    .filter(|rect| rect.is_within(frame.width, frame.height));
                match dirty {
                    Some(rect) if contiguous => UploadPlan::PartialUpload(rect),
                    _ => UploadPlan::Upload,
                }
            }
        }
    }

    /// 记录已完成的上传（`RecreateAndUpload` / `Upload` / `PartialUpload` 成功后调用）。
    ///
    /// 局部上传成功也记同一份 `(size, version)`：**版本即内容标识**，与本次写了
    /// 多少像素无关——只有全部写入成功才允许推进版本，失败方负责 `reset`。
    pub(super) fn record(&mut self, frame: &CpuOnionFrame) {
        self.size = Some(frame.size());
        self.version = Some(frame.version);
    }

    /// 复位（驻留纹理被释放或状态自相矛盾时调用）。
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// 已上传的内容版本。
    pub(super) fn version(&self) -> Option<u64> {
        self.version
    }

    /// 已驻留纹理的尺寸。
    #[cfg(test)]
    pub(super) fn size(&self) -> Option<(u32, u32)> {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, version: u64) -> CpuOnionFrame {
        let len = CpuOnionFrame::expected_len(width, height).unwrap_or(0);
        CpuOnionFrame::new(0, 0, width, height, vec![0u8; len], version)
    }

    #[test]
    fn test_cpu_onion_upload_plan_skips_invalid_frames() {
        let state = UploadState::default();
        assert_eq!(state.plan(&frame(0, 0, 1)), UploadPlan::Skip);
        let bad = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; 15], 1);
        assert_eq!(state.plan(&bad), UploadPlan::Skip);
    }

    #[test]
    fn test_cpu_onion_upload_plan_recreates_on_first_frame() {
        let state = UploadState::default();
        assert_eq!(state.plan(&frame(4, 3, 1)), UploadPlan::RecreateAndUpload);
    }

    #[test]
    fn test_cpu_onion_upload_plan_skips_same_version_and_size() {
        let mut state = UploadState::default();
        let f = frame(4, 3, 10);
        assert_eq!(state.plan(&f), UploadPlan::RecreateAndUpload);
        state.record(&f);
        assert_eq!(state.version(), Some(10));
        assert_eq!(state.size(), Some((4, 3)));

        // 尺寸与版本都相同 → 不重复上传
        assert_eq!(state.plan(&f), UploadPlan::Reuse);
        assert_eq!(state.plan(&frame(4, 3, 10)), UploadPlan::Reuse);
    }

    #[test]
    fn test_cpu_onion_upload_plan_uploads_on_version_change() {
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));
        assert_eq!(state.plan(&frame(4, 3, 11)), UploadPlan::Upload);
        assert_eq!(
            state.version(),
            Some(10),
            "只推导动作，不得提前改写已上传版本"
        );
    }

    #[test]
    fn test_cpu_onion_upload_plan_recreates_on_size_change() {
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));
        // 尺寸变化必须重建纹理，哪怕版本号相同
        assert_eq!(state.plan(&frame(8, 3, 10)), UploadPlan::RecreateAndUpload);
        assert_eq!(state.plan(&frame(4, 6, 10)), UploadPlan::RecreateAndUpload);
    }

    #[test]
    fn test_cpu_onion_upload_plan_reset_requires_recreate() {
        let mut state = UploadState::default();
        let f = frame(2, 2, 1);
        state.record(&f);
        state.reset();
        assert_eq!(state.version(), None);
        assert_eq!(state.size(), None);
        assert_eq!(state.plan(&f), UploadPlan::RecreateAndUpload);
    }

    /// 带脏区的帧（脏区为帧内像素坐标，与 `x`/`y` 无关）。
    fn frame_with_dirty(
        width: u32,
        height: u32,
        version: u64,
        dirty: Option<CpuOnionUploadRect>,
    ) -> CpuOnionFrame {
        frame(width, height, version).with_dirty_rect(dirty)
    }

    #[test]
    fn test_cpu_onion_upload_plan_partial_upload_on_valid_dirty() {
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));

        let rect = CpuOnionUploadRect::new(1, 1, 2, 1);
        assert_eq!(
            state.plan(&frame_with_dirty(4, 3, 11, Some(rect))),
            UploadPlan::PartialUpload(rect),
            "尺寸相同 + 版本变化 + 脏区完整落在帧内 → 只写该子矩形"
        );
        // 推导动作不得提前推进版本（写入成功由 `record` 负责）
        assert_eq!(state.version(), Some(10));
    }

    #[test]
    fn test_cpu_onion_upload_plan_falls_back_to_full_on_version_gap() {
        // 渲染侧三缓冲会合并中间帧：版本跳号（1 → 3）说明中间那次区域重绘从未落进
        // 纹理，此时只写本帧脏区会永久留下旧像素 → 必须全量。
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 1));
        let rect = CpuOnionUploadRect::new(1, 1, 2, 1);
        assert_eq!(
            state.plan(&frame_with_dirty(4, 3, 3, Some(rect))),
            UploadPlan::Upload,
            "版本跳号必须全量上传（脏区只对紧邻前一版有效）"
        );
    }

    #[test]
    fn test_cpu_onion_upload_plan_ignores_dirty_when_recreating() {
        // 首帧（无驻留纹理）即便带脏区也必须整帧铺满
        let state = UploadState::default();
        let rect = CpuOnionUploadRect::new(0, 0, 1, 1);
        assert_eq!(
            state.plan(&frame_with_dirty(4, 3, 1, Some(rect))),
            UploadPlan::RecreateAndUpload
        );

        // 尺寸变化同理：新纹理没有「上一版像素」可保留，脏区无意义
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));
        assert_eq!(
            state.plan(&frame_with_dirty(8, 6, 11, Some(rect))),
            UploadPlan::RecreateAndUpload
        );
    }

    #[test]
    fn test_cpu_onion_upload_plan_reuses_same_version_ignoring_dirty() {
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));
        // 同版本 = 内容同一份：脏区标注不得触发任何上传
        assert_eq!(
            state.plan(&frame_with_dirty(
                4,
                3,
                10,
                Some(CpuOnionUploadRect::new(1, 1, 2, 1))
            )),
            UploadPlan::Reuse
        );
    }

    #[test]
    fn test_cpu_onion_upload_plan_falls_back_to_full_on_invalid_dirty() {
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 10));

        for dirty in [
            // 空矩形
            Some(CpuOnionUploadRect::new(1, 1, 0, 2)),
            Some(CpuOnionUploadRect::new(1, 1, 2, 0)),
            // 右侧越界（2 + 4 > 4）
            Some(CpuOnionUploadRect::new(2, 1, 4, 1)),
            // 下侧越界（1 + 4 > 3）
            Some(CpuOnionUploadRect::new(1, 1, 1, 4)),
            // 起点越界
            Some(CpuOnionUploadRect::new(4, 0, 1, 1)),
            Some(CpuOnionUploadRect::new(0, 3, 1, 1)),
            // 饱和溢出
            Some(CpuOnionUploadRect::new(0, 0, u32::MAX, 1)),
            // 无脏区
            None,
        ] {
            assert_eq!(
                state.plan(&frame_with_dirty(4, 3, 11, dirty)),
                UploadPlan::Upload,
                "脏区不可信时必须保守全量上传：{dirty:?}"
            );
        }
    }

    #[test]
    fn test_cpu_onion_upload_plan_records_partial_upload() {
        let mut state = UploadState::default();
        let first = frame(4, 3, 10);
        state.record(&first);

        let rect = CpuOnionUploadRect::new(1, 1, 2, 1);
        let partial = frame_with_dirty(4, 3, 11, Some(rect));
        assert_eq!(state.plan(&partial), UploadPlan::PartialUpload(rect));

        // 局部写入成功后记录：版本/尺寸照常推进，下一帧同版本直接 Reuse
        state.record(&partial);
        assert_eq!(state.version(), Some(11));
        assert_eq!(state.size(), Some((4, 3)));
        assert_eq!(state.plan(&partial), UploadPlan::Reuse);
    }

    #[test]
    fn test_cpu_onion_upload_plan_full_upload_on_reenabled_session() {
        // 场景：CPU 会话 A 上传 version = 1 → 切 GPU（渲染侧 UploadState 仍驻留
        // (4,3)/1）→ 切回 CPU，新会话版本严格递增到 2，且首帧是整帧重绘（无脏区）。
        // 必须走全量 Upload：若版本回退成 1，这里会被误判为 Reuse 而保留旧纹理。
        let mut state = UploadState::default();
        state.record(&frame(4, 3, 1));
        assert_eq!(
            state.plan(&frame_with_dirty(4, 3, 2, None)),
            UploadPlan::Upload,
            "新会话版本递增 + 无脏区 → 必须全量上传，不能复用上一会话纹理"
        );
        // 反证：版本回退会被误判成复用
        assert_eq!(
            state.plan(&frame_with_dirty(4, 3, 1, None)),
            UploadPlan::Reuse
        );
    }
}
