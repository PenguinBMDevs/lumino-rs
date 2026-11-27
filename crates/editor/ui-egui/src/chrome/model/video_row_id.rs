//! 视频导出的行标识（typed id）。
//!
//! `VideoRow` / `VideoExportAction` 之前用裸 `u16`，dispatcher 里 65 个魔数分支
//! 无法被编译器检查。这里给行模型与动作统一加一层新类型。

/// 视频导出控件的行标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct VideoRowId(u16);

impl VideoRowId {
    /// 由原始值构造（常量表使用）。
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// 原始值（仅诊断 / 兼容旧接口）。
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl From<u16> for VideoRowId {
    fn from(raw: u16) -> Self {
        Self(raw)
    }
}

impl From<VideoRowId> for u16 {
    fn from(id: VideoRowId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_raw_id() {
        assert_eq!(VideoRowId::from(101).get(), 101);
        assert_eq!(u16::from(VideoRowId::new(236)), 236);
    }
}
