//! 非有限样本净化 — 渲染主循环与尾部收尾共用口径（从 processor.rs 拆出）

/// 非有限样本（NaN/Inf）按静音净化。
///
/// 返回 `(净化数量, 首个非有限样本偏移)`。
///
/// **渲染主循环与尾部收尾必须共用本函数**：两处口径分叉正是 REND-002 尾部
/// NaN 旁路的根因——尾部直接写 sink，漏净化时限幅器关闭会把 NaN/Inf 直写 WAV；
/// 且 `is_silent` 的 `abs() < eps` 判据对 NaN 恒为 false，污染尾部会让收尾
/// 循环跑满批次上限（120s 垃圾）。
pub(crate) fn purify_non_finite(buffer: &mut [f32]) -> (u64, Option<usize>) {
    let mut bad = 0_u64;
    let mut first_bad: Option<usize> = None;
    for (i, sample) in buffer.iter_mut().enumerate() {
        if !sample.is_finite() {
            if first_bad.is_none() {
                first_bad = Some(i);
            }
            *sample = 0.0;
            bad += 1;
        }
    }
    (bad, first_bad)
}
