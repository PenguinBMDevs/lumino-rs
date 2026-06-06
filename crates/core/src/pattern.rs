//! Pattern 数据结构 —— 音轨总览中的音符片段
//!
/// 音轨总览视图中的 Pattern（音符片段）
#[derive(Debug, Clone)]
pub struct Pattern {
    /// 唯一标识
    pub id: u32,
    /// 所属音轨 ID
    pub track_id: usize,
    /// Pattern 名称
    pub name: String,
    /// 起始时间位置（tick）
    pub start_tick: f32,
    /// 长度（tick）
    pub length: f32,
    /// RGBA 颜色
    pub color: [f32; 4],
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            id: 0,
            track_id: 0,
            name: String::from("Pattern"),
            start_tick: 0.0,
            length: 960.0,               // 默认一小节（4/4 拍，PPQ=240）
            color: [0.2, 0.6, 1.0, 1.0], // 默认蓝色
        }
    }
}

impl Pattern {
    /// 创建新的 Pattern
    pub fn new(id: u32, track_id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            track_id,
            name: name.into(),
            ..Default::default()
        }
    }

    /// 设置起始位置
    pub fn with_start_tick(mut self, start_tick: f32) -> Self {
        self.start_tick = start_tick;
        self
    }

    /// 设置长度
    pub fn with_length(mut self, length: f32) -> Self {
        self.length = length;
        self
    }

    /// 设置颜色
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// 结束 tick 位置
    pub fn end_tick(&self) -> f32 {
        self.start_tick + self.length
    }

    /// 判断 Pattern 是否与给定 tick 范围重叠
    pub fn overlaps(&self, range_start: f32, range_end: f32) -> bool {
        self.start_tick < range_end && self.end_tick() > range_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_default() {
        let pattern = Pattern::default();
        assert_eq!(pattern.id, 0);
        assert_eq!(pattern.track_id, 0);
        assert_eq!(pattern.name, "Pattern");
        assert!((pattern.start_tick - 0.0).abs() < f32::EPSILON);
        assert!((pattern.length - 960.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pattern_builder() {
        let pattern = Pattern::new(1, 2, "Notes1")
            .with_start_tick(480.0)
            .with_length(1920.0)
            .with_color([0.5, 0.3, 0.9, 1.0]);

        assert_eq!(pattern.id, 1);
        assert_eq!(pattern.track_id, 2);
        assert_eq!(pattern.name, "Notes1");
        assert!((pattern.start_tick - 480.0).abs() < f32::EPSILON);
        assert!((pattern.length - 1920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_end_tick() {
        let pattern = Pattern::new(0, 0, "")
            .with_start_tick(100.0)
            .with_length(500.0);
        let result = pattern.end_tick();
        assert!((result - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_overlaps() {
        let pattern = Pattern::new(0, 0, "")
            .with_start_tick(100.0)
            .with_length(200.0);

        // 完全在范围内
        assert!(pattern.overlaps(50.0, 400.0));
        // 左边界重叠
        assert!(pattern.overlaps(50.0, 150.0));
        // 右边界重叠
        assert!(pattern.overlaps(250.0, 400.0));
        // 不重叠（左侧）
        assert!(!pattern.overlaps(0.0, 50.0));
        // 不重叠（右侧）
        assert!(!pattern.overlaps(350.0, 500.0));
    }
}
