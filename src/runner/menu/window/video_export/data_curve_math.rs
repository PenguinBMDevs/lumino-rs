//! 数据曲线模式数值工具（原版 MIDIGraphRenderer 函数移植）
//!
//! - `is_milestone`：里程碑刻度判定（0 或 >=1000 的 10 的整数次幂）
//! - `add_commas`：千分位逗号格式化
//! - `abbreviate`：数字缩写（K/M/B/T）
//! - `smooth_forward`：前向滑动平均（O(n) 滑动和，替代原版 O(n²)）

/// RGBA → BGRA 帧内像素序。
pub(super) fn rgba_to_bgra(c: [u8; 4]) -> [u8; 4] {
    [c[2], c[1], c[0], c[3]]
}

/// 是否里程碑刻度（原版 `isMilestone`）：0 或 >= start 的 10 的整数次幂。
pub(super) fn is_milestone(num: f64, milestone_start: f64) -> bool {
    let n = num.floor();
    if n == 0.0 {
        return true;
    }
    if n < milestone_start {
        return false;
    }
    // 整数判定：不断除 10，剩 1 则是 10 的幂（避免浮点 log10 精度问题）
    let mut m = n as u64;
    while m.is_multiple_of(10) {
        m /= 10;
    }
    m == 1
}

/// 千分位逗号格式化（原版 `addCommas`）。
pub(super) fn add_commas(num: f64) -> String {
    let n = num.floor() as i64;
    let digits = n.abs().to_string();
    let sign = if n < 0 { "-" } else { "" };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    out.push_str(sign);
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// 数字缩写（原版 `abbreviate`）：>=1000 → K/M/B/T 后缀。
///
/// `digit_scale` 为调用方预计算的 `10^decimal_places`（避免每帧求幂）。
pub(super) fn abbreviate(num: f64, digit_scale: f64) -> String {
    let mut v = num;
    let mut suffix = "";
    if v.abs() >= 1000.0 {
        v /= 1000.0;
        suffix = "K";
    }
    if v.abs() >= 1000.0 {
        v /= 1000.0;
        suffix = "M";
    }
    if v.abs() >= 1000.0 {
        v /= 1000.0;
        suffix = "B";
    }
    if v.abs() >= 1000.0 {
        v /= 1000.0;
        suffix = "T";
    }
    let m = digit_scale.max(1.0);
    let trimmed = (v * m).floor() / m;
    format!("{trimmed}{suffix}")
}

/// 前向滑动平均（原版 `smoothout`+`avg_slice`，O(n) 滑动和替代 O(n²)）。
///
/// `out[i] = avg(values[i..=i+smoothness])`：右端未越界时窗口长度保持
/// `smoothness+1`；右端越界后窗口逐帧收缩（与原版 `avg_slice` 截断语义一致）。
pub(super) fn smooth_forward(values: &[f64], smoothness: usize) -> Vec<f64> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let s = smoothness.min(n - 1);
    let mut out = vec![0.0; n];
    let mut sum: f64 = values[..=s].iter().sum();
    let mut cnt = s + 1;
    out[0] = sum / cnt as f64;
    for i in 1..n {
        sum -= values[i - 1]; // 左端移出
        if i + s < n {
            sum += values[i + s]; // 右端移入，窗口长度不变
        } else {
            cnt -= 1; // 右端到顶，窗口收缩
        }
        out[i] = sum / cnt.max(1) as f64;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milestone() {
        assert!(is_milestone(0.0, 1000.0));
        assert!(!is_milestone(999.0, 1000.0));
        assert!(is_milestone(1000.0, 1000.0));
        assert!(is_milestone(10_000.0, 1000.0));
        assert!(!is_milestone(1500.0, 1000.0));
        assert!(is_milestone(1_000_000.0, 1000.0));
    }

    #[test]
    fn test_add_commas() {
        assert_eq!(add_commas(0.0), "0");
        assert_eq!(add_commas(999.0), "999");
        assert_eq!(add_commas(1000.0), "1,000");
        assert_eq!(add_commas(1_234_567.0), "1,234,567");
        assert_eq!(add_commas(-1234.0), "-1,234");
    }

    #[test]
    fn test_abbreviate() {
        let scale = 10.0f64.powi(3).floor();
        assert_eq!(abbreviate(999.0, scale), "999");
        assert_eq!(abbreviate(1500.0, scale), "1.5K");
        assert_eq!(abbreviate(2_000_000.0, scale), "2M");
        assert_eq!(abbreviate(3_500_000_000.0, scale), "3.5B");
        assert_eq!(abbreviate(4_200_000_000_000.0, scale), "4.2T");
    }

    #[test]
    fn test_smooth_forward_matches_bruteforce() {
        let data = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
        let s = 2;
        let fast = smooth_forward(&data, s);
        // 暴力参照（原版 avg_slice 语义）
        let brute: Vec<f64> = (0..data.len())
            .map(|i| {
                let end = (i + s).min(data.len() - 1);
                data[i..=end].iter().sum::<f64>() / (end - i + 1) as f64
            })
            .collect();
        assert_eq!(fast, brute);
    }

    #[test]
    fn test_smooth_forward_empty() {
        let empty: [f64; 0] = [];
        assert!(smooth_forward(&empty, 2).is_empty());
    }
}
