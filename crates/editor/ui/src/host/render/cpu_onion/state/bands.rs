//! CPU 洋葱皮 band 并行度（`state.rs` 子模块）。

/// band 数上限（每 band 一条 tokio worker 线程）。
///
/// 6 = 示例默认物理核（i7-9750H 6c）/ 常见 6 核档位；1080p 2M 音符实测：
/// Exact 4 band 29.0ms → 6 band 22.4ms（-23%）。再高（8）在示例上已回退，
/// 且每 band 一块 1080p 缓冲，内存/调度收益失衡。
const MAX_BANDS: usize = 6;

/// 诊断覆盖：`LUMINO_CPU_ONION_BANDS=1..=6`（示例 `--workers` 的生产等价物）。
const BANDS_ENV: &str = "LUMINO_CPU_ONION_BANDS";

/// 按可用并行度取 band 数，限制在 `1..=MAX_BANDS`；环境变量可覆盖。
pub(super) fn worker_bands() -> usize {
    if let Some(value) = std::env::var_os(BANDS_ENV)
        && let Some(parsed) = value
            .to_str()
            .and_then(|raw| parse_band_override(Some(raw)))
    {
        return parsed;
    }
    std::thread::available_parallelism()
        .map_or(2, |parallelism| parallelism.get())
        .clamp(1, MAX_BANDS)
}

/// 解析 band 覆盖值；越界/非数字返回 `None`（回退自动探测）。
fn parse_band_override(raw: Option<&str>) -> Option<usize> {
    let parsed = raw?.trim().parse::<usize>().ok()?;
    (1..=MAX_BANDS).contains(&parsed).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::{MAX_BANDS, parse_band_override, worker_bands};

    #[test]
    fn band_override_accepts_only_in_range_values() {
        assert_eq!(parse_band_override(Some("1")), Some(1));
        assert_eq!(parse_band_override(Some("6")), Some(6));
        assert_eq!(parse_band_override(Some(" 4 ")), Some(4));
        assert_eq!(parse_band_override(Some("0")), None);
        assert_eq!(
            parse_band_override(Some(&(MAX_BANDS + 1).to_string())),
            None
        );
        assert_eq!(parse_band_override(Some("x")), None);
        assert_eq!(parse_band_override(None), None);
        assert!((1..=MAX_BANDS).contains(&worker_bands()));
    }
}
