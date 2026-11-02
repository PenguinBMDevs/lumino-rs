//! CPU 洋葱皮 band 并行度（`state.rs` 子模块）。

/// band 数上限（每 band 一条 tokio worker 线程）。
///
/// 12 = i7-9750H 6c/12t 的逻辑核数；Paprika 310M 真实语料实测
/// 6 band 127.5fps → 12 band 142.8fps（p50 6.25 → 5.64ms），16 band 回退到
/// 131.2fps。低核机器由 `available_parallelism` 自动下调。
const MAX_BANDS: usize = 12;

/// 诊断覆盖：`LUMINO_CPU_ONION_BANDS=1..=12`（示例 `--workers` 的生产等价物）。
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
