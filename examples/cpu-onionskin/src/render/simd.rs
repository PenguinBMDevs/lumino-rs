//! SIMD 原语复用共享 crate（T-32b）。
//!
//! 实现已迁移到 lumino-cpu-onion::simd；示例只保留 re-export，避免第二份分叉。

pub use lumino_cpu_onion::simd::{SimdLevel, fill_row, masked_fill_row};
