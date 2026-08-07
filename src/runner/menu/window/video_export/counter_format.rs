//! 计数器模式数字格式化辅助
//!
//! 格式语义参考 Zenith-MIDI NoteCountRender 的 .NET 复合格式
//! （`ToString("#,00000")` 千分位 + 补零、`"00.0000"` 百分比等）。

use lumino_event::window::video::CounterSeparator;

/// 将整数按 千分位 + 可选补零 格式化为字符串。
///
/// 语义参考 Zenith `.ToString("#,00000")`：先左补零到 `pad` 位，
/// 再从右向左每 3 位插入分隔符（`use_comma` 时）。
pub(super) fn format_int(v: u64, pad: u32, use_comma: bool, zeroes: bool) -> String {
    let mut digits = v.to_string();
    if zeroes && pad > digits.len() as u32 {
        digits = format!("{:0>pad$}", digits, pad = pad as usize);
    }
    if use_comma && digits.len() > 3 {
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(ch);
        }
        digits = out;
    }
    digits
}

/// 将浮点数按 整数补零 + 小数位数 + 千分位 格式化为字符串（BPM 用）。
pub(super) fn format_float(
    v: f64,
    int_pad: u32,
    dec_pad: u32,
    use_comma: bool,
    zeroes: bool,
) -> String {
    let dec = dec_pad.min(12) as usize;
    let fixed = format!("{v:.dec$}", dec = dec);
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let int_fmt = format_int(
        int_part.parse::<u64>().unwrap_or(0),
        int_pad,
        use_comma,
        zeroes,
    );
    if dec == 0 {
        int_fmt
    } else {
        format!("{int_fmt}.{frac_part}")
    }
}

/// 将秒数格式化为 mm:ss
pub(super) fn fmt_mmss(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (m, s) = (total / 60, total % 60);
    format!("{m:02}:{s:02}")
}

/// 将秒数格式化为 mm:ss.fff
pub(super) fn fmt_mmss_fff(secs: f64) -> String {
    let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
    let (m, s, ms) = (total_ms / 60_000, (total_ms / 1000) % 60, total_ms % 1000);
    format!("{m:02}:{s:02}.{ms:03}")
}

/// 百分比格式（Zenith 风格 "00.0000"：整数至少 2 位 + 4 位小数）。
pub(super) fn format_percent(numerator: f64, denominator: f64) -> String {
    if denominator <= 0.0 {
        return "00.0000".to_string();
    }
    let pct = numerator / denominator * 100.0;
    format!("{pct:07.4}")
}

/// 千分位开关（供模板替换使用）。
pub(super) fn use_comma(separator: CounterSeparator) -> bool {
    separator == CounterSeparator::Comma
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 千分位 + 补零（.NET "#,00000" 语义：先补零后插入千分位）
    #[test]
    fn test_format_int() {
        assert_eq!(format_int(1234, 5, true, true), "01,234");
        assert_eq!(format_int(1234, 5, false, true), "01234");
        assert_eq!(format_int(1234, 5, true, false), "1,234");
        assert_eq!(format_int(123, 5, true, false), "123");
        assert_eq!(format_int(0, 5, true, true), "00,000");
        assert_eq!(format_int(0, 5, false, true), "00000");
    }

    /// BPM 浮点格式化
    #[test]
    fn test_format_float() {
        assert_eq!(format_float(120.0, 3, 2, false, true), "120.00");
        assert_eq!(format_float(5.5, 3, 2, false, true), "005.50");
        assert_eq!(format_float(1234.567, 4, 2, true, true), "1,234.57");
        assert_eq!(format_float(120.0, 3, 0, false, false), "120");
    }

    /// 时间格式化
    #[test]
    fn test_time_format() {
        assert_eq!(fmt_mmss(65.0), "01:05");
        assert_eq!(fmt_mmss_fff(65.25), "01:05.250");
        assert_eq!(fmt_mmss_fff(0.0), "00:00.000");
    }

    /// 百分比格式
    #[test]
    fn test_format_percent() {
        assert_eq!(format_percent(1.0, 2.0), "50.0000");
        assert_eq!(format_percent(0.0, 0.0), "00.0000", "除零保护");
        assert_eq!(format_percent(1.0, 4.0), "25.0000");
    }
}
