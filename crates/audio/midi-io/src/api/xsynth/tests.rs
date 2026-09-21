use crate::realtime::ThreadCount as LuminoThreadCount;

use super::threads::{available_parallelism, forced_thread_count, parse_thread_pool_override};
use super::{THREAD_POOL_ENV, normalize_max_voices_per_key};

#[test]
fn thread_policy_never_uses_channel_pool_by_default() {
    // 关键回归：任何核数都必须走「无池 + 批渲染」，
    // 不得再按 >16 逻辑核切到「共享池 + 关批渲染」的历史分支。
    for cores in [1usize, 2, 4, 6, 12, 16, 17, 24, 32, 64] {
        assert_eq!(
            parse_thread_pool_override(None, cores),
            LuminoThreadCount::None,
            "{cores} 逻辑核默认必须无通道内池"
        );
    }
    assert!(available_parallelism() >= 1, "逻辑核数应至少为 1");
    // 未设置逃生口时，真实解析路径同样恒为无池。
    if std::env::var(THREAD_POOL_ENV).is_err() {
        assert_eq!(forced_thread_count(32), LuminoThreadCount::None);
    }
}

#[test]
fn thread_pool_override_parses_only_whitelisted_values() {
    // 只有显式白名单才允许切回池化（A/B 复测用）；其余一律保持默认无池。
    for value in [
        None,
        Some(""),
        Some("   "),
        Some("0"),
        Some("none"),
        Some("off"),
        Some("false"),
        Some("bogus"),
        Some("-1"),
    ] {
        assert_eq!(
            parse_thread_pool_override(value, 24),
            LuminoThreadCount::None,
            "{value:?} 应保持默认无池"
        );
    }
    for value in [Some("manual"), Some("on"), Some("true"), Some(" manual ")] {
        assert_eq!(
            parse_thread_pool_override(value, 24),
            LuminoThreadCount::Manual(24),
            "{value:?} 应强制池化"
        );
    }
    assert_eq!(
        parse_thread_pool_override(Some("8"), 24),
        LuminoThreadCount::Manual(8)
    );
    // 异常环境（逻辑核数 0）也不得构造出合法的 Manual(0)。
    assert_eq!(
        parse_thread_pool_override(Some("manual"), 0),
        LuminoThreadCount::Manual(1)
    );
}

#[test]
fn normalize_max_voices_per_key_bounds() {
    assert_eq!(normalize_max_voices_per_key(None), None);
    assert_eq!(
        normalize_max_voices_per_key(Some(0)),
        None,
        "0 按不限制处理，绝不能下发 Some(0)"
    );
    assert_eq!(normalize_max_voices_per_key(Some(1)), Some(1));
    assert_eq!(normalize_max_voices_per_key(Some(16)), Some(16));
    assert_eq!(
        normalize_max_voices_per_key(Some(200)),
        Some(128),
        "上限应夹紧到 128"
    );
}
