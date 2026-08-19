//! 编辑器状态相关常量
//!
//! 这些常量是编辑器状态初始化和交互计算的基础阈值，集中管理以避免魔法数扩散。

/// 默认 BPM（用于新文档初始化和重置）
pub const DEFAULT_BPM: f64 = 120.0;

/// 默认预览音符力度（点击/绘制音符时的试听力度）
pub const DEFAULT_PREVIEW_VELOCITY: u8 = 100;

/// 选择框边缘命中阈值（像素）
pub const SELECTION_BOX_EDGE_THRESHOLD: f32 = 4.0;

/// 音符合并邻近阈值（tick）
pub const GLUE_PROXIMITY_THRESHOLD: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bpm_value() {
        const { assert!(DEFAULT_BPM == 120.0) };
    }

    #[test]
    fn test_default_preview_velocity_value() {
        const { assert!(DEFAULT_PREVIEW_VELOCITY == 100) };
    }

    #[test]
    fn test_selection_box_edge_threshold_positive() {
        const { assert!(SELECTION_BOX_EDGE_THRESHOLD > 0.0) };
    }

    #[test]
    fn test_glue_proximity_threshold_non_negative() {
        const { assert!(GLUE_PROXIMITY_THRESHOLD >= 0.0) };
    }
}
