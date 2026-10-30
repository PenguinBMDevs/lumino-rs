//! 像素填充 SIMD 原语（AVX2，运行时检测，标量兜底）。
//!
//! 音符是轴对齐矩形 + 每侧 2px 同色描边、纯色填充；用 8×u32 的 AVX2 store
//! 填充行是 CPU 光栅最直接的可向量化点。覆盖判定用 per-pixel owner
//! （0 = 空，1 = 洋葱皮）实现 GPU 的稳定深度语义：同级先到先得（first-wins）。

/// SIMD 档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimdLevel {
    /// AVX2（x86_64）。
    Avx2,
    /// 标量兜底。
    Scalar,
}

impl SimdLevel {
    /// 运行时检测；非 x86_64 或 CPU 不支持 AVX2 时回退标量。
    ///
    /// 诊断覆盖：`LUMINO_CPU_ONION_SCALAR=1` 强制标量（示例 `--no-simd` 的
    /// 生产等价物，用于 A/B 与排查向量化回归）。
    #[must_use]
    pub(crate) fn detect() -> Self {
        if std::env::var_os("LUMINO_CPU_ONION_SCALAR").is_some() {
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
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Avx2 => "avx2",
            Self::Scalar => "scalar",
        }
    }
}

/// 按 owner 条件填充：只有 `owner <= owner_limit` 的像素才写入，
/// 并把这些像素的 owner 设为 `owner_value`。
///
/// 洋葱皮固定使用 `owner_value = 1, owner_limit = 0`：先画的音符占住像素后，
/// 后画的音符不能覆盖（与 GPU 洋葱皮区按全局源索引稳定排序的深度语义一致）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn masked_fill_row(
    row: &mut [u32],
    owner: &mut [u32],
    x0: i32,
    x1: i32,
    value: u32,
    owner_value: u32,
    owner_limit: u32,
    level: SimdLevel,
) {
    let len = row.len().min(owner.len());
    let row = &mut row[..len];
    let owner = &mut owner[..len];
    let start = (x0.max(0) as usize).min(len);
    let end = (x1.max(0) as usize).min(len);
    if start >= end {
        return;
    }
    let dst = &mut row[start..end];
    let own = &mut owner[start..end];
    match level {
        SimdLevel::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            // SAFETY: 仅在本机确认 AVX2 后由 `detect()` 返回该档位；dst/own
            // 是等长合法切片，循环内不越界。
            unsafe {
                masked_fill_row_avx2(dst, own, value, owner_value, owner_limit);
            }
            #[cfg(not(target_arch = "x86_64"))]
            masked_fill_row_scalar(dst, own, value, owner_value, owner_limit);
        }
        SimdLevel::Scalar => {
            masked_fill_row_scalar(dst, own, value, owner_value, owner_limit);
        }
    }
}

fn masked_fill_row_scalar(
    dst: &mut [u32],
    owner: &mut [u32],
    value: u32,
    owner_value: u32,
    owner_limit: u32,
) {
    for (pixel, slot) in dst.iter_mut().zip(owner.iter_mut()) {
        if *slot <= owner_limit {
            *pixel = value;
            *slot = owner_value;
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
) {
    use std::arch::x86_64::{
        __m256i, _mm256_blendv_epi8, _mm256_cmpgt_epi32, _mm256_loadu_si256,
        _mm256_maskstore_epi32, _mm256_movemask_epi8, _mm256_set1_epi32, _mm256_storeu_si256,
    };

    let len = dst.len();
    let dst_ptr = dst.as_mut_ptr();
    let owner_ptr = owner.as_mut_ptr();
    let color = _mm256_set1_epi32(value as i32);
    let new_owner = _mm256_set1_epi32(owner_value as i32);
    let limit = _mm256_set1_epi32(owner_limit.saturating_add(1) as i32);
    let mut i = 0usize;
    // SAFETY: 每轮 i + 8 <= len；load/store 均在切片长度内，maskstore 的掩码
    // 只选择已加载的 8 个 lane。
    unsafe {
        while i + 8 <= len {
            let owners = _mm256_loadu_si256(owner_ptr.add(i) as *const __m256i);
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
    masked_fill_row_scalar(
        &mut dst[i..],
        &mut owner[i..],
        value,
        owner_value,
        owner_limit,
    );
}

/// `0xAARRGGBB` 行 → RGBA8 字节（每像素 R,G,B,A）。
///
/// 1080p 输出帧有 207 万像素，标量逐像素位移约 3~4ms；AVX2 用 `pshufb`
/// 把每个 4 字节组 `[B,G,R,A]`（小端）重排为 `[R,G,B,A]`，降到亚毫秒。
/// `dst` 长度不足时按 `dst.len() / 4` 截断，绝不越界。
pub(crate) fn argb_to_rgba_row(src: &[u32], dst: &mut [u8], level: SimdLevel) {
    let len = src.len().min(dst.len() / 4);
    if len == 0 {
        return;
    }
    let src = &src[..len];
    let dst = &mut dst[..len * 4];
    match level {
        SimdLevel::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            // SAFETY: 仅在本机确认 AVX2 后由 `detect()` 返回该档位；src/dst
            // 已按像素数截断，循环条件 `index + 8 <= len` 保证 32 字节读写不越界。
            unsafe {
                argb_to_rgba_avx2(src, dst);
            }
            #[cfg(not(target_arch = "x86_64"))]
            argb_to_rgba_scalar(src, dst);
        }
        SimdLevel::Scalar => argb_to_rgba_scalar(src, dst),
    }
}

fn argb_to_rgba_scalar(src: &[u32], dst: &mut [u8]) {
    for (chunk, &pixel) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src) {
        *chunk = [
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
            ((pixel >> 24) & 0xFF) as u8,
        ];
    }
}

/// # Safety
///
/// 调用方必须确认 CPU 支持 AVX2，且 `dst` 至少有 `src.len() * 4` 字节。
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn argb_to_rgba_avx2(src: &[u32], dst: &mut [u8]) {
    use std::arch::x86_64::{
        __m256i, _mm256_loadu_si256, _mm256_setr_epi8, _mm256_shuffle_epi8, _mm256_storeu_si256,
    };
    let len = src.len().min(dst.len() / 4);
    // 每个 128-bit lane 内做同样的 4 字节重排：[B,G,R,A] → [R,G,B,A]。
    let mask = _mm256_setr_epi8(
        2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15, 2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11,
        14, 13, 12, 15,
    );
    let mut index = 0;
    // SAFETY: 见函数 `# Safety`；`index + 8 <= len` 保证 32 字节读写不越界。
    unsafe {
        while index + 8 <= len {
            let value = _mm256_loadu_si256(src.as_ptr().add(index) as *const __m256i);
            let shuffled = _mm256_shuffle_epi8(value, mask);
            _mm256_storeu_si256(dst.as_mut_ptr().add(index * 4) as *mut __m256i, shuffled);
            index += 8;
        }
    }
    argb_to_rgba_scalar(&src[index..len], &mut dst[index * 4..len * 4]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_fill_only_touches_allowed_lanes() {
        let mut row = [0u32; 24];
        let mut owner = [0u32; 24];
        for (index, slot) in owner.iter_mut().enumerate() {
            *slot = (index % 3) as u32;
        }
        masked_fill_row(&mut row, &mut owner, 0, 24, 0xABCD, 1, 0, SimdLevel::Scalar);
        for index in 0..24 {
            if index % 3 == 0 {
                assert_eq!(row[index], 0xABCD);
                assert_eq!(owner[index], 1);
            } else {
                assert_eq!(row[index], 0);
            }
        }
    }

    #[test]
    fn masked_fill_clips_to_row_bounds() {
        let mut row = [0u32; 10];
        let mut owner = [0u32; 10];
        masked_fill_row(&mut row, &mut owner, -5, 3, 7, 1, 0, SimdLevel::Scalar);
        assert_eq!(row[..3], [7, 7, 7]);
        assert_eq!(row[3..], [0; 7]);
        masked_fill_row(&mut row, &mut owner, 8, 99, 9, 1, 0, SimdLevel::Scalar);
        assert_eq!(row[8..], [9, 9]);
    }

    #[test]
    fn simd_level_name_is_stable() {
        assert_eq!(SimdLevel::Scalar.name(), "scalar");
        assert_eq!(SimdLevel::Avx2.name(), "avx2");
        assert!(matches!(
            SimdLevel::detect(),
            SimdLevel::Avx2 | SimdLevel::Scalar
        ));
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
        masked_fill_row(
            &mut scalar_row,
            &mut scalar_owner,
            2,
            35,
            0x1234,
            1,
            1,
            SimdLevel::Scalar,
        );
        masked_fill_row(
            &mut simd_row,
            &mut simd_owner,
            2,
            35,
            0x1234,
            1,
            1,
            SimdLevel::Avx2,
        );
        assert_eq!(scalar_row, simd_row);
        assert_eq!(scalar_owner, simd_owner);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_argb_to_rgba_matches_scalar() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let pixels: Vec<u32> = (0..37u32)
            .map(|index| 0xA1_00_00_00 | (index.wrapping_mul(0x0003_0507) & 0x00FF_FFFF))
            .collect();
        let mut scalar = vec![0u8; pixels.len() * 4];
        let mut simd = vec![0u8; pixels.len() * 4];
        argb_to_rgba_row(&pixels, &mut scalar, SimdLevel::Scalar);
        argb_to_rgba_row(&pixels, &mut simd, SimdLevel::Avx2);
        assert_eq!(scalar, simd, "AVX2 字节重排必须与标量逐字节一致");
        // 37 像素 = 32 个走 AVX2 + 5 个走标量尾。
        assert_eq!(scalar[..4], [0, 0, 0, 0xA1]);
        assert_eq!(scalar.len(), 37 * 4);
    }

    #[test]
    fn argb_to_rgba_row_truncates_to_shorter_buffer() {
        let pixels = [0x11_22_33_44u32; 4];
        let mut dst = [0u8; 8];
        argb_to_rgba_row(&pixels, &mut dst, SimdLevel::Scalar);
        assert_eq!(dst, [0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44, 0x11]);
    }
}
