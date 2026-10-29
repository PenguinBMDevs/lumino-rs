//! 上传决策状态机（纯逻辑，不依赖 GPU，便于单元测试）。
//!
//! CPU 洋葱皮本步采用**整帧全量上传**（不做局部脏区）：只有尺寸变化（需重建纹理）
//! 或 `version` 变化（内容变化）才上传；尺寸与版本都相同则直接复用上一帧纹理。

use super::frame::CpuOnionFrame;

/// 一帧对应的纹理上传动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UploadPlan {
    /// 帧无效（尺寸为 0 / 像素长度不匹配）：不绘制、不上传
    Skip,
    /// 驻留纹理尺寸与帧尺寸不同：重建纹理并全量上传
    RecreateAndUpload,
    /// 尺寸相同但内容版本变化：全量上传（复用纹理）
    Upload,
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
    /// 计算本帧需要执行的上传动作。
    pub(super) fn plan(&self, frame: &CpuOnionFrame) -> UploadPlan {
        if !frame.validate() {
            return UploadPlan::Skip;
        }
        match self.size {
            // 尚无驻留纹理
            None => UploadPlan::RecreateAndUpload,
            // 尺寸变化：旧纹理无法承载新帧
            Some(size) if size != frame.size() => UploadPlan::RecreateAndUpload,
            // 尺寸相同、内容版本变化
            Some(_) if self.version != Some(frame.version) => UploadPlan::Upload,
            // 尺寸与版本都相同
            Some(_) => UploadPlan::Reuse,
        }
    }

    /// 记录已完成的上传（`RecreateAndUpload` 与 `Upload` 成功后调用）。
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
}
