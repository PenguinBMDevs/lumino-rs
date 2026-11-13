//! i2m 面板参数测试（DRAW-030）

use super::*;
use crate::right_sidebar::i2m_tables::*;
use lumino_message::{I2mConfigField, I2mSelectField};

#[test]
fn test_default_matches_i2m_rs() {
    let cfg = I2mConfig::default();
    let cc = cfg.to_converter_config();
    assert_eq!(cc.start_key, 21);
    assert_eq!(cc.end_key, 108);
    assert_eq!(cc.color_count, 16);
    assert_eq!(cc.ticks_per_pixel, 1);
    assert_eq!(cc.target_height, 0);
    assert!(matches!(cc.palette, PaletteSource::KMeansPlusPlus));
    // 新增字段必须取「不改变输出」的默认值
    assert_eq!(cc.key_mode, KeyMode::AllKeys);
    assert_eq!(cc.note_length_mode, NoteLengthMode::Unlimited);
    assert_eq!(cc.max_note_length, 0);
    assert_eq!(cc.resize_algorithm, ResizeAlgorithm::AreaResampling);
    assert_eq!(cc.color_id_method, ColorIdMethod::Rgb);
    assert_eq!(cfg.blend(), I2mBlend::None);
}

#[test]
fn test_apply_text_rejects_non_digits() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::TicksPerPixel, "abc");
    assert_eq!(cfg.ticks_per_pixel, 1);
    assert_eq!(cfg.ticks_per_pixel_text, "1");
    cfg.apply_text(I2mConfigField::Brightness, "1.5");
    assert_eq!(cfg.brightness, 0);
}

#[test]
fn test_apply_text_accepts_negative_percent() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::Brightness, "-25");
    assert_eq!(cfg.brightness, -25);
    assert_eq!(cfg.brightness_text, "-25");
    cfg.apply_text(I2mConfigField::Contrast, "-999");
    assert_eq!(cfg.contrast, -100, "越界应夹取");
    assert_eq!(cfg.contrast_text, "-100");
}

#[test]
fn test_single_minus_keeps_input_buffer() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::Saturation, "-");
    assert_eq!(cfg.saturation, 0, "单独负号不改数值");
    assert_eq!(cfg.saturation_text, "-", "保留输入缓冲继续编辑");
}

#[test]
fn test_apply_text_clamps_ranges() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::StartKey, "200");
    assert_eq!(cfg.start_key, 127);
    cfg.apply_text(I2mConfigField::TicksPerPixel, "0");
    assert_eq!(cfg.ticks_per_pixel, 1);
    cfg.apply_text(I2mConfigField::ColorCount, "0");
    assert_eq!(cfg.color_count, 1);
    cfg.apply_text(I2mConfigField::TargetHeight, "9999");
    assert_eq!(cfg.target_height, 2048);
    cfg.apply_text(I2mConfigField::MaxNoteLength, "99999");
    assert_eq!(cfg.max_note_length, 4096);
    cfg.apply_text(I2mConfigField::Sharpen, "999");
    assert_eq!(cfg.sharpen, 100);
    cfg.apply_text(I2mConfigField::Sharpen, "-5");
    assert_eq!(cfg.sharpen, 0);
}

#[test]
fn test_apply_text_keeps_start_le_end() {
    let mut cfg = I2mConfig::default();
    // start 超过 end → end 联动提升，保持 start <= end
    cfg.apply_text(I2mConfigField::StartKey, "120");
    assert_eq!(cfg.start_key, 120);
    assert_eq!(cfg.end_key, 120);
    // end 低于 start → start 联动压低
    cfg.apply_text(I2mConfigField::EndKey, "30");
    assert_eq!(cfg.end_key, 30);
    assert_eq!(cfg.start_key, 30);
}

#[test]
fn test_empty_text_keeps_value() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::TicksPerPixel, "");
    assert_eq!(cfg.ticks_per_pixel, 1);
    assert_eq!(cfg.ticks_per_pixel_text, "");
}

#[test]
fn test_to_converter_config_normalizes_key_order() {
    let cfg = I2mConfig {
        start_key: 100,
        end_key: 50,
        ..Default::default()
    };
    let cc = cfg.to_converter_config();
    assert_eq!(cc.start_key, 50);
    assert_eq!(cc.end_key, 100);
}

#[test]
fn test_palette_fallback_on_out_of_range_index() {
    let cfg = I2mConfig {
        palette_index: 999,
        ..Default::default()
    };
    assert!(matches!(cfg.palette(), PaletteSource::KMeansPlusPlus));
}

#[test]
fn test_blend_overrides_palette_algorithm() {
    let mut cfg = I2mConfig::default();
    cfg.set_selection(I2mSelectField::Blend, 1); // spread
    assert!(matches!(cfg.palette(), PaletteSource::FloydSteinbergDither));
    cfg.set_selection(I2mSelectField::Blend, 2); // pattern
    assert!(matches!(cfg.palette(), PaletteSource::OrderedDither));
    // none → 回到所选调色板算法
    cfg.set_selection(I2mSelectField::Blend, 0);
    cfg.set_selection(I2mSelectField::Palette, 3); // octree
    assert!(matches!(cfg.palette(), PaletteSource::Octree));
}

#[test]
fn test_set_selection_rejects_out_of_range() {
    let mut cfg = I2mConfig::default();
    let before = cfg.key_mode_index;
    cfg.set_selection(I2mSelectField::KeyMode, 999);
    assert_eq!(cfg.key_mode_index, before, "越界索引不应写入");
}

#[test]
fn test_to_converter_config_wires_new_fields() {
    let mut cfg = I2mConfig::default();
    cfg.set_selection(I2mSelectField::KeyMode, 1); // 白键（填充）
    cfg.set_selection(I2mSelectField::NoteLengthMode, 2); // 按网格切分
    cfg.set_selection(I2mSelectField::ResizeAlgorithm, 2); // 最近邻
    cfg.set_selection(I2mSelectField::ColorIdMethod, 3); // CIEDE2000
    cfg.apply_text(I2mConfigField::MaxNoteLength, "12");

    let cc = cfg.to_converter_config();
    assert_eq!(cc.key_mode, KeyMode::WhiteKeysFilled);
    assert_eq!(cc.note_length_mode, NoteLengthMode::SplitToGrid);
    assert_eq!(cc.max_note_length, 12);
    assert_eq!(cc.resize_algorithm, ResizeAlgorithm::NearestNeighbor);
    assert_eq!(cc.color_id_method, ColorIdMethod::Ciede2000);
}

#[test]
fn test_adjust_percent_to_unit_range() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::Brightness, "-50");
    cfg.apply_text(I2mConfigField::Contrast, "25");
    cfg.apply_text(I2mConfigField::Saturation, "100");
    cfg.apply_text(I2mConfigField::Sharpen, "40");
    let adjust = cfg.adjust();
    assert!((adjust.brightness + 0.5).abs() < 1e-6);
    assert!((adjust.contrast - 0.25).abs() < 1e-6);
    assert!((adjust.saturation - 1.0).abs() < 1e-6);
    assert!((adjust.sharpen - 0.4).abs() < 1e-6);
    assert_eq!(
        I2mConfig::default().adjust(),
        lumino_extras::Adjust::default()
    );
}

#[test]
fn test_settings_roundtrip() {
    let mut cfg = I2mConfig::default();
    cfg.apply_text(I2mConfigField::StartKey, "36");
    cfg.apply_text(I2mConfigField::ColorCount, "8");
    cfg.apply_text(I2mConfigField::Brightness, "-20");
    cfg.apply_text(I2mConfigField::MaxNoteLength, "24");
    cfg.set_selection(I2mSelectField::Palette, 3);
    cfg.set_selection(I2mSelectField::KeyMode, 2);
    cfg.set_selection(I2mSelectField::NoteLengthMode, 1);
    cfg.set_selection(I2mSelectField::ResizeAlgorithm, 4);
    cfg.set_selection(I2mSelectField::ColorIdMethod, 2);
    cfg.set_selection(I2mSelectField::Blend, 1);

    let settings = cfg.to_settings();
    let restored = I2mConfig::from_settings(&settings);
    assert_eq!(restored.start_key, 36);
    assert_eq!(restored.color_count, 8);
    assert_eq!(restored.brightness, -20);
    assert_eq!(restored.max_note_length, 24);
    assert_eq!(restored.palette_index, 3);
    assert_eq!(restored.key_mode_index, 2);
    assert_eq!(restored.note_length_mode_index, 1);
    assert_eq!(restored.resize_algorithm_index, 4);
    assert_eq!(restored.color_id_method_index, 2);
    assert_eq!(restored.blend_index, 1);
    // 文本缓冲同步，重开工程面板显示正确
    assert_eq!(restored.start_key_text, "36");
    assert_eq!(restored.brightness_text, "-20");
}

#[test]
fn test_settings_unknown_ids_fall_back_to_default() {
    let settings = lumino_core::I2mSettings {
        palette: "no_such_palette".into(),
        key_mode: "no_such_mode".into(),
        blend: "no_such_blend".into(),
        ..lumino_core::I2mSettings::default()
    };
    let cfg = I2mConfig::from_settings(&settings);
    assert_eq!(cfg.palette_index, 0);
    assert_eq!(cfg.key_mode_index, 0);
    assert_eq!(cfg.blend_index, 0);
}

#[test]
fn test_settings_values_are_clamped() {
    let settings = lumino_core::I2mSettings {
        start_key: 200,
        ticks_per_pixel: 0,
        color_count: 0,
        brightness: 999,
        sharpen: -10,
        ..lumino_core::I2mSettings::default()
    };
    let cfg = I2mConfig::from_settings(&settings);
    assert_eq!(cfg.start_key, 127);
    assert_eq!(cfg.ticks_per_pixel, 1);
    assert_eq!(cfg.color_count, 1);
    assert_eq!(cfg.brightness, 100);
    assert_eq!(cfg.sharpen, 0);
}

#[test]
fn test_stable_ids_are_distinct() {
    // 持久化依赖 id 唯一；重复 id 会让恢复落到错误选项
    for table in [
        palette_algorithms()
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        key_modes().iter().map(|(id, _, _)| *id).collect(),
        note_length_modes().iter().map(|(id, _, _)| *id).collect(),
        resize_algorithms().iter().map(|(id, _, _)| *id).collect(),
        color_id_methods().iter().map(|(id, _, _)| *id).collect(),
        blend_modes().iter().map(|(id, _, _)| *id).collect(),
    ] {
        let mut sorted = table.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), table.len(), "稳定 id 必须唯一：{table:?}");
    }
}
