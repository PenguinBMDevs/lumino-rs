//! 音符量化功能
//!
//! 提供将音符时间对齐到网格的功能，支持可配置的量化强度。
//! 核心算法与 UI 解耦，可独立测试和复用。

/// 量化配置
#[derive(Debug, Clone)]
pub struct QuantizeConfig {
    /// 网格大小（ticks）
    pub grid_size: f32,
    /// 量化强度 (0.0-1.0)，1.0=完全对齐
    pub strength: f32,
    /// 是否量化起始时间
    pub quantize_start: bool,
    /// 是否量化结束时间
    pub quantize_end: bool,
}

impl Default for QuantizeConfig {
    fn default() -> Self {
        Self {
            grid_size: 480.0,
            strength: 1.0,
            quantize_start: true,
            quantize_end: false,
        }
    }
}

impl QuantizeConfig {
    /// 创建新的量化配置
    pub fn new(grid_size: f32, strength: f32) -> Self {
        let strength = strength.clamp(0.0, 1.0);
        Self {
            grid_size,
            strength,
            quantize_start: true,
            quantize_end: false,
        }
    }

    /// 设置是否量化起始时间
    pub fn with_start(mut self, quantize: bool) -> Self {
        self.quantize_start = quantize;
        self
    }

    /// 设置是否量化结束时间
    pub fn with_end(mut self, quantize: bool) -> Self {
        self.quantize_end = quantize;
        self
    }
}

/// 量化单个 tick 值
///
/// 将 tick 对齐到最近的网格点（使用四舍五入，0.5向上取整），strength 控制对齐程度：
/// - 0.0 = 不变
/// - 1.0 = 完全对齐到最近的网格点
///
/// # 参数
/// - `tick`: 原始 tick 值
/// - `grid_size`: 网格大小（ticks）
/// - `strength`: 量化强度 (0.0-1.0)
///
/// # 返回值
/// 量化后的 tick 值
pub fn quantize_tick(tick: f32, grid_size: f32, strength: f32) -> f32 {
    if grid_size <= 0.0 || strength <= 0.0 {
        return tick;
    }

    let strength = strength.clamp(0.0, 1.0);

    let quotient = tick / grid_size;
    let rounded = if quotient >= 0.0 {
        (quotient + 0.5).floor()
    } else {
        (quotient - 0.5).ceil()
    } * grid_size;

    tick + (rounded - tick) * strength
}

/// 音符数据结构（用于量化操作的内部表示）
#[derive(Debug, Clone)]
pub struct QuantizableNote {
    /// 起始时间（tick）
    pub tick: f32,
    /// 音符长度（tick）
    pub length: f32,
}

impl QuantizableNote {
    pub fn new(tick: f32, length: f32) -> Self {
        Self { tick, length }
    }
}

/// 对单个可量化音符执行量化
fn quantize_single_note(note: &mut QuantizableNote, config: &QuantizeConfig) -> bool {
    let mut modified = false;

    if config.quantize_start {
        let original_tick = note.tick;
        note.tick = quantize_tick(note.tick, config.grid_size, config.strength);
        if (note.tick - original_tick).abs() > f32::EPSILON {
            modified = true;
        }
    }

    if config.quantize_end {
        let original_end = note.tick + note.length;
        let new_end = quantize_tick(original_end, config.grid_size, config.strength);
        let new_length = new_end - note.tick;

        if (new_length - note.length).abs() > f32::EPSILON && new_length > 0.0 {
            note.length = new_length;
            modified = true;
        }
    }

    modified
}

/// 对音符列表执行批量量化
///
/// # 参数
/// - `notes`: 可量化音符列表（可变引用）
/// - `config`: 量化配置
///
/// # 返回值
/// 被修改的音符数量
pub fn quantize_notes(notes: &mut [QuantizableNote], config: &QuantizeConfig) -> usize {
    if notes.is_empty() || config.grid_size <= 0.0 {
        return 0;
    }

    let mut count = 0;
    for note in notes.iter_mut() {
        if quantize_single_note(note, config) {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_note(tick: f32, length: f32) -> QuantizableNote {
        QuantizableNote::new(tick, length)
    }

    #[test]
    fn test_quantize_tick_basic() {
        let grid_size = 480.0;

        let quantized = quantize_tick(100.0, grid_size, 1.0);
        assert!((quantized - 0.0).abs() < f32::EPSILON);

        let quantized = quantize_tick(240.0, grid_size, 1.0);
        // 240 正好在中间，四舍五入应该向上取整到 480
        assert!((quantized - 480.0).abs() < f32::EPSILON);

        let quantized = quantize_tick(260.0, grid_size, 1.0);
        assert!((quantized - 480.0).abs() < f32::EPSILON);

        let quantized = quantize_tick(480.0, grid_size, 1.0);
        assert!((quantized - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_quantize_strength() {
        let grid_size = 480.0;
        let tick = 240.0;

        let quantized_0 = quantize_tick(tick, grid_size, 0.0);
        assert!((quantized_0 - 240.0).abs() < f32::EPSILON);

        let quantized_05 = quantize_tick(tick, grid_size, 0.5);
        // 完全量化(1.0)→480，50%强度: 240 + (480-240)*0.5 = 360
        assert!((quantized_05 - 360.0).abs() < f32::EPSILON);

        let quantized_1 = quantize_tick(tick, grid_size, 1.0);
        // 四舍五入：240正好在中间，向上取整到480
        assert!((quantized_1 - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_quantize_grid_sizes() {
        let tick = 500.0;

        let quantized_whole = quantize_tick(tick, 1920.0, 1.0);
        // 500/1920 = 0.26，四舍五入→0
        assert!((quantized_whole - 0.0).abs() < f32::EPSILON);

        let quantized_half = quantize_tick(tick, 960.0, 1.0);
        // 500/960 = 0.52，四舍五入→1，所以是 1*960=960
        assert!((quantized_half - 960.0).abs() < f32::EPSILON);

        let quantized_quarter = quantize_tick(tick, 480.0, 1.0);
        // 500/480 = 1.04，四舍五入→1，所以是 1*480=480
        assert!((quantized_quarter - 480.0).abs() < f32::EPSILON);

        let quantized_eighth = quantize_tick(tick, 240.0, 1.0);
        // 500/240 = 2.08，四舍五入→2，所以是 2*240=480
        assert!((quantized_eighth - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_quantize_notes() {
        let mut notes = vec![
            create_test_note(100.0, 480.0),
            create_test_note(250.0, 240.0),
            create_test_note(480.0, 480.0),
        ];

        let config = QuantizeConfig::new(480.0, 1.0);
        let count = quantize_notes(&mut notes, &config);

        assert_eq!(count, 2);
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[1].tick - 480.0).abs() < f32::EPSILON);
        assert!((notes[2].tick - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_quantize_with_end() {
        let mut notes = vec![create_test_note(100.0, 500.0)];

        let config = QuantizeConfig::new(480.0, 1.0)
            .with_start(true)
            .with_end(true);
        let count = quantize_notes(&mut notes, &config);

        assert_eq!(count, 1);
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_edge_cases() {
        let quantized_zero = quantize_tick(0.0, 480.0, 1.0);
        assert!((quantized_zero - 0.0).abs() < f32::EPSILON);

        let quantized_negative = quantize_tick(-100.0, 480.0, 1.0);
        assert!((quantized_negative - 0.0).abs() < f32::EPSILON);

        let quantized_large = quantize_tick(100000.0, 480.0, 1.0);
        let expected: f32 = (100000.0_f32 / 480.0_f32).round() * 480.0_f32;
        assert!((quantized_large - expected).abs() < f32::EPSILON);

        let quantized_zero_grid = quantize_tick(100.0, 0.0, 1.0);
        assert!((quantized_zero_grid - 100.0).abs() < f32::EPSILON);

        let quantized_zero_strength = quantize_tick(100.0, 480.0, 0.0);
        assert!((quantized_zero_strength - 100.0).abs() < f32::EPSILON);

        let mut empty_notes: Vec<QuantizableNote> = vec![];
        let config = QuantizeConfig::default();
        let count = quantize_notes(&mut empty_notes, &config);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_default_config() {
        let config = QuantizeConfig::default();
        assert!((config.grid_size - 480.0).abs() < f32::EPSILON);
        assert!((config.strength - 1.0).abs() < f32::EPSILON);
        assert!(config.quantize_start);
        assert!(!config.quantize_end);
    }

    #[test]
    fn test_quantize_preserves_unmodified_notes() {
        let mut notes = vec![
            create_test_note(480.0, 480.0),
            create_test_note(960.0, 240.0),
        ];

        let config = QuantizeConfig::new(480.0, 1.0);
        let count = quantize_notes(&mut notes, &config);

        assert_eq!(count, 0);
        assert!((notes[0].tick - 480.0).abs() < f32::EPSILON);
        assert!((notes[1].tick - 960.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_high_precision_grid() {
        let grid_size = 30.0;
        let tick = 37.0;

        let quantized = quantize_tick(tick, grid_size, 1.0);
        assert!((quantized - 30.0).abs() < f32::EPSILON);

        let tick2 = 44.0;
        let quantized2 = quantize_tick(tick2, grid_size, 1.0);
        assert!((quantized2 - 30.0).abs() < f32::EPSILON);

        let tick3 = 46.0;
        let quantized3 = quantize_tick(tick3, grid_size, 1.0);
        assert!((quantized3 - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_partial_strength_interpolation() {
        let grid_size = 480.0;
        let tick = 120.0;

        let quantized_25 = quantize_tick(tick, grid_size, 0.25);
        let expected_25 = tick + (0.0 - tick) * 0.25;
        assert!((quantized_25 - expected_25).abs() < f32::EPSILON);

        let quantized_75 = quantize_tick(tick, grid_size, 0.75);
        let expected_75 = tick + (0.0 - tick) * 0.75;
        assert!((quantized_75 - expected_75).abs() < f32::EPSILON);
    }
}
