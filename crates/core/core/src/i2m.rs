//! 图片转 MIDI 的工程级参数（DRAW-030）
//!
//! 存放在 `.lmpj` 的 `[settings.i2m]` 段，随工程保存/重开恢复。
//! 只存整数与**稳定 id 字符串**，不依赖 `i2m-rs` 的枚举：
//! 上游版本增删枚举变体时，旧工程按 id 回退到默认项，而不是整段解析失败。

/// 图片转 MIDI 参数（工程级持久化）
///
/// 运算单位与面板一致：
/// - `brightness` / `contrast` / `saturation`：百分比 `-100..=100`
/// - `sharpen`：百分比 `0..=100`
/// - `max_note_length`：像素行（`0` = 不限），与 `i2m-rs` 同口径
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct I2mSettings {
    /// 最低 MIDI key（0-127）
    pub start_key: u8,
    /// 最高 MIDI key（0-127）
    pub end_key: u8,
    /// 目标高度（像素，0 = 保持宽高比）
    pub target_height: u32,
    /// 每像素行对应的 MIDI tick（>0）
    pub ticks_per_pixel: u32,
    /// 调色板颜色数（= 生成音轨数）
    pub color_count: u32,
    /// 最长音符（像素行，0 = 不限）
    pub max_note_length: u32,
    /// 亮度百分比（-100..=100）
    pub brightness: i16,
    /// 对比度百分比（-100..=100）
    pub contrast: i16,
    /// 饱和度百分比（-100..=100）
    pub saturation: i16,
    /// 锐化百分比（0..=100）
    pub sharpen: i16,
    /// 调色板算法稳定 id
    pub palette: String,
    /// key 映射稳定 id
    pub key_mode: String,
    /// 音符长度策略稳定 id
    pub note_length_mode: String,
    /// 重采样算法稳定 id
    pub resize_algorithm: String,
    /// 颜色匹配口径稳定 id
    pub color_id_method: String,
    /// 混合模式稳定 id（spread / pattern / none）
    pub blend: String,
}

impl Default for I2mSettings {
    /// 与 `i2m-rs` `ConverterConfig::default()` 对齐，外观与接线前逐音符一致。
    fn default() -> Self {
        Self {
            start_key: 21,
            end_key: 108,
            target_height: 0,
            ticks_per_pixel: 1,
            color_count: 16,
            max_note_length: 0,
            brightness: 0,
            contrast: 0,
            saturation: 0,
            sharpen: 0,
            palette: "kmeans_plus_plus".into(),
            key_mode: "all_keys".into(),
            note_length_mode: "unlimited".into(),
            resize_algorithm: "area_resampling".into(),
            color_id_method: "rgb".into(),
            blend: "none".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_matches_i2m_rs_defaults() {
        let s = I2mSettings::default();
        assert_eq!((s.start_key, s.end_key), (21, 108));
        assert_eq!(s.color_count, 16);
        assert_eq!(s.ticks_per_pixel, 1);
        assert_eq!(s.target_height, 0);
        assert_eq!(s.max_note_length, 0);
        assert_eq!(
            (s.brightness, s.contrast, s.saturation, s.sharpen),
            (0, 0, 0, 0)
        );
        assert_eq!(s.palette, "kmeans_plus_plus");
        assert_eq!(s.key_mode, "all_keys");
        assert_eq!(s.note_length_mode, "unlimited");
        assert_eq!(s.resize_algorithm, "area_resampling");
        assert_eq!(s.color_id_method, "rgb");
        assert_eq!(s.blend, "none");
    }

    #[test]
    fn test_missing_fields_fall_back_to_default() {
        // `#[serde(default)]` 保证旧工程只有部分字段时仍能解析
        let decoded: I2mSettings =
            toml::from_str("start_key = 36\ncolor_count = 8\n").expect("部分字段应能解析");
        assert_eq!(decoded.start_key, 36);
        assert_eq!(decoded.color_count, 8);
        assert_eq!(decoded.end_key, 108);
        assert_eq!(decoded.blend, "none");
    }

    #[test]
    fn test_roundtrip_keeps_all_fields() {
        let s = I2mSettings {
            start_key: 36,
            end_key: 96,
            color_count: 4,
            brightness: -25,
            sharpen: 60,
            palette: "octree".into(),
            blend: "spread".into(),
            ..I2mSettings::default()
        };
        let text = toml::to_string(&s).expect("序列化失败");
        let decoded: I2mSettings = toml::from_str(&text).expect("反序列化失败");
        assert_eq!(decoded, s);
    }
}
