//! 像素填充 SIMD 原语（AVX2，运行时检测，标量兜底）。
//!
//! 音符是轴对齐矩形 + 每侧 2px 同色描边，纯色填充；用 8×u32 的 AVX2 store
//! 填充行/背景，是 CPU 光栅最直接的可向量化点。覆盖判定用 per-pixel owner
//! （0=空，1=洋葱皮，2=主轨）实现 GPU 的稳定深度语义：主轨覆盖洋葱皮，
//! 同级先到先得。

/// SIMD 档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    /// AVX2（x86_64）。
    // 非 x86_64 上 `detect()` 永远不会返回该档位（机器码路径本身也已 cfg 掉），
    // 枚举值仍需保留给测试与 `name()` 做跨平台断言；CI 的 `-D warnings`
    // 会把「从未构造」升为编译错误，故按目标架构局部豁免。
    #[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
    Avx2,
    /// 标量兜底。
    Scalar,
}

impl SimdLevel {
    /// 运行时检测；`force_scalar` 用于 A/B 基准。
    #[must_use]
    pub fn detect(force_scalar: bool) -> Self {
        if force_scalar {
            return Self::Scalar;
        }
        #[cfg(target_arch = "x86_64")]
        {
            if std::is_x86_feature_detected!("avx2") {
                return Self::Avx2;
            }
        }
        Self::Scalar
    }

    /// 可读名称。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Avx2 => "avx2",
            Self::Scalar => "scalar",
        }
    }
}

/// 用纯色填充 `row[x0..x1]`（x1 排他），自动裁剪。
pub fn fill_row(row: &mut [u32], x0: i32, x1: i32, value: u32, level: SimdLevel) {
    let len = row.len() as i32;
    let start = x0.clamp(0, len) as usize;
    let end = x1.clamp(0, len) as usize;
    if start >= end {
        return;
    }
    let dst = &mut row[start..end];
    match level {
        SimdLevel::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            // SAFETY: 仅在本机确认 AVX2 后调用；dst 是合法切片，循环内不越界。
            unsafe {
                fill_row_avx2(dst, value);
            }
            #[cfg(not(target_arch = "x86_64"))]
            dst.fill(value);
        }
        SimdLevel::Scalar => dst.fill(value),
    }
}

/// 按 owner 条件填充：只有 `owner <= owner_limit` 的像素才写入，
/// 并把这些像素的 owner 设为 `owner_value`。
///
/// 洋葱皮：`owner_value=1, owner_limit=0`；主轨：`owner_value=2, owner_limit=1`。
///
/// `count_coverage` 为真时返回本段中**原本 owner==0** 的像素数，供洋葱皮
/// pass 判断"画布是否已经铺满、后续轨能否整轨跳过"；为假时返回 0，不付
/// 额外统计开销。
#[allow(clippy::too_many_arguments)]
pub fn masked_fill_row(
    row: &mut [u32],
    owner: &mut [u32],
    x0: i32,
    x1: i32,
    value: u32,
    owner_value: u32,
    owner_limit: u32,
    level: SimdLevel,
    count_coverage: bool,
) -> usize {
    debug_assert_eq!(row.len(), owner.len());
    let len = row.len() as i32;
    let start = x0.clamp(0, len) as usize;
    let end = x1.clamp(0, len) as usize;
    if start >= end {
        return 0;
    }
    let dst = &mut row[start..end];
    let own = &mut owner[start..end];
    match level {
        SimdLevel::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            // SAFETY: 仅在本机确认 AVX2 后调用；dst/own 等长切片，循环内不越界。
            unsafe {
                masked_fill_row_avx2(dst, own, value, owner_value, owner_limit, count_coverage)
            }
            #[cfg(not(target_arch = "x86_64"))]
            masked_fill_row_scalar(dst, own, value, owner_value, owner_limit, count_coverage)
        }
        SimdLevel::Scalar => {
            masked_fill_row_scalar(dst, own, value, owner_value, owner_limit, count_coverage)
        }
    }
}

fn masked_fill_row_scalar(
    dst: &mut [u32],
    owner: &mut [u32],
    value: u32,
    owner_value: u32,
    owner_limit: u32,
    count_coverage: bool,
) -> usize {
    let mut covered = 0usize;
    for (pixel, slot) in dst.iter_mut().zip(owner.iter_mut()) {
        if *slot <= owner_limit {
            if count_coverage && *slot == 0 {
                covered += 1;
            }
            *pixel = value;
            *slot = owner_value;
        }
    }
    covered
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fill_row_avx2(dst: &mut [u32], value: u32) {
    use std::arch::x86_64::{__m256i, _mm256_set1_epi32, _mm256_storeu_si256};

    let len = dst.len();
    let ptr = dst.as_mut_ptr();
    let vector = _mm256_set1_epi32(value as i32);
    let mut i = 0usize;
    // SAFETY: i + 8 <= len，ptr 派生自长度 len 的切片；写入均为 u32 对齐。
    unsafe {
        while i + 8 <= len {
            _mm256_storeu_si256(ptr.add(i) as *mut __m256i, vector);
            i += 8;
        }
        while i < len {
            *ptr.add(i) = value;
            i += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn masked_fill_row_avx2(
    dst: &mut [u32],
    owner: &mut [u32],
    value: u32,
    owner_value: u32,
    owner_limit: u32,
    count_coverage: bool,
) -> usize {
    use std::arch::x86_64::{
        __m256i, _mm256_blendv_epi8, _mm256_castsi256_ps, _mm256_cmpeq_epi32, _mm256_cmpgt_epi32,
        _mm256_loadu_si256, _mm256_maskstore_epi32, _mm256_movemask_epi8, _mm256_movemask_ps,
        _mm256_set1_epi32, _mm256_setzero_si256, _mm256_storeu_si256,
    };

    let len = dst.len();
    let dst_ptr = dst.as_mut_ptr();
    let owner_ptr = owner.as_mut_ptr();
    let color = _mm256_set1_epi32(value as i32);
    let new_owner = _mm256_set1_epi32(owner_value as i32);
    let limit = _mm256_set1_epi32(owner_limit.saturating_add(1) as i32);
    let zero = _mm256_setzero_si256();
    let mut covered = 0usize;
    let mut i = 0usize;
    // SAFETY: 每轮 i + 8 <= len；load/store 均在切片长度内，maskstore 的掩码
    // 只选择已加载的 8 个 lane。
    unsafe {
        while i + 8 <= len {
            let owners = _mm256_loadu_si256(owner_ptr.add(i) as *const __m256i);
            if count_coverage {
                let zero_mask = _mm256_cmpeq_epi32(owners, zero);
                covered += (_mm256_movemask_ps(_mm256_castsi256_ps(zero_mask)) as u32).count_ones()
                    as usize;
            }
            let mask = _mm256_cmpgt_epi32(limit, owners);
            let bits = _mm256_movemask_epi8(mask);
            if bits == -1 {
                _mm256_storeu_si256(dst_ptr.add(i) as *mut __m256i, color);
                _mm256_storeu_si256(owner_ptr.add(i) as *mut __m256i, new_owner);
            } else if bits != 0 {
                _mm256_maskstore_epi32(dst_ptr.add(i) as *mut i32, mask, color);
                let merged = _mm256_blendv_epi8(owners, new_owner, mask);
                _mm256_storeu_si256(owner_ptr.add(i) as *mut __m256i, merged);
            }
            i += 8;
        }
    }
    covered
        + masked_fill_row_scalar(
            &mut dst[i..],
            &mut owner[i..],
            value,
            owner_value,
            owner_limit,
            count_coverage,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_row_clips() {
        let mut row = [0u32; 20];
        fill_row(&mut row, -3, 4, 7, SimdLevel::Scalar);
        assert_eq!(row[..4], [7, 7, 7, 7]);
        assert_eq!(row[4], 0);
        fill_row(&mut row, 18, 99, 9, SimdLevel::Scalar);
        assert_eq!(row[18..], [9, 9]);
    }

    #[test]
    fn masked_fill_only_touches_allowed_lanes() {
        let mut row = [0u32; 24];
        let mut owner = [0u32; 24];
        for (index, slot) in owner.iter_mut().enumerate() {
            *slot = (index % 3) as u32;
        }
        let covered = masked_fill_row(
            &mut row,
            &mut owner,
            0,
            24,
            0xABCD,
            1,
            0,
            SimdLevel::Scalar,
            true,
        );
        assert_eq!(covered, 8, "只有 owner==0 的 8 个像素计入覆盖");
        for index in 0..24 {
            if index % 3 == 0 {
                assert_eq!(row[index], 0xABCD);
                assert_eq!(owner[index], 1);
            } else {
                assert_eq!(row[index], 0);
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_matches_scalar_when_available() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut scalar_row = [0u32; 37];
        let mut scalar_owner = [0u32; 37];
        let mut simd_row = [0u32; 37];
        let mut simd_owner = [0u32; 37];
        for index in 0..37 {
            scalar_owner[index] = (index % 4) as u32;
            simd_owner[index] = scalar_owner[index];
        }
        let scalar_covered = masked_fill_row(
            &mut scalar_row,
            &mut scalar_owner,
            2,
            35,
            0x1234,
            1,
            1,
            SimdLevel::Scalar,
            true,
        );
        let simd_covered = masked_fill_row(
            &mut simd_row,
            &mut simd_owner,
            2,
            35,
            0x1234,
            1,
            1,
            SimdLevel::Avx2,
            true,
        );
        assert_eq!(scalar_row, simd_row);
        assert_eq!(scalar_owner, simd_owner);
        assert_eq!(scalar_covered, simd_covered);
    }
}
