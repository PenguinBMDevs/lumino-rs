//! 逐像素差异统计与报告（从 `harness` 拆出，保持每个文件 < 400 行）。
//!
//! 比较口径见 [`compare`]：双方透明背景不计入；一方覆盖另一方不覆盖记几何差异；
//! 双方覆盖必须 alpha = 255，并比较 RGB（超出 [`COLOR_TOLERANCE`] 才记差异）。

use lumino_cpu_onion::CpuFrame;

use super::harness::{H, W};

/// RGB 单通道容差（0..=255 刻度）。
///
/// CPU 侧用 `srgb_encode` 自己量化，GPU 侧由硬件做 linear→sRGB 写入：两路取色
/// 同源、8 位量化在**硬件**后端逐位相同（Metal / 独显 Vulkan/DX12 实测 0 差），
/// 但**软件光栅**（CI 的 Linux lavapipe、Windows WARP）在 linear→sRGB 转换上
/// 会做不同的舍入，实测最多差 1。几何（覆盖集合）与 alpha 仍要求逐像素严格
/// 一致，只有颜色放宽这 1 级，避免把后端舍入差异当成渲染语义不一致。
pub const COLOR_TOLERANCE: u8 = 1;

/// 单个差异像素。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diff {
    /// 列。
    pub x: u32,
    /// 行。
    pub y: u32,
    /// CPU 像素（RGBA8）。
    pub cpu: [u8; 4],
    /// GPU 像素（RGBA8）。
    pub gpu: [u8; 4],
}

/// 逐像素差异统计（背景双方都透明时不计入比较）。
#[derive(Clone, Debug, Default)]
pub struct Diffs {
    /// 总像素数。
    pub total: usize,
    /// 双方都覆盖（alpha != 0）并进入颜色比较的像素数。
    pub covered: usize,
    /// 覆盖集合不一致（一边有音符、一边是背景）的像素。
    pub geometry: Vec<Diff>,
    /// 双方都覆盖但 RGB 不一致的像素。
    pub color: Vec<Diff>,
    /// 覆盖像素中 alpha != 255 的个数（CPU + GPU 合计）。
    pub alpha_violations: Vec<Diff>,
    /// 颜色落在 [`COLOR_TOLERANCE`] 内、不计入失败但需要可见的像素数。
    pub tolerated: usize,
    /// RGB 单通道最大差值（含容差内差异，仅供报告）。
    pub max_channel_delta: u8,
}

impl Diffs {
    /// 是否完全一致（覆盖集合、RGB、alpha 全部相同）。
    pub fn is_exact(&self) -> bool {
        self.geometry.is_empty() && self.color.is_empty() && self.alpha_violations.is_empty()
    }

    /// 一行摘要（含前若干差异坐标）。
    pub fn summary(&self) -> String {
        let mut text = format!(
            "画布 {}×{} = {} 像素｜双方覆盖 {}｜几何差异 {}｜颜色差异 {}（容差内 {}）｜最大通道差 {}｜alpha 违规 {}",
            W,
            H,
            self.total,
            self.covered,
            self.geometry.len(),
            self.color.len(),
            self.tolerated,
            self.max_channel_delta,
            self.alpha_violations.len()
        );
        let coords: Vec<String> = self
            .geometry
            .iter()
            .chain(self.color.iter())
            .take(12)
            .map(|d| format!("({},{}) cpu={:?} gpu={:?}", d.x, d.y, d.cpu, d.gpu))
            .collect();
        if !coords.is_empty() {
            text.push_str("｜前若干差异：");
            text.push_str(&coords.join("; "));
        }
        text
    }
}

/// 逐像素比较 CPU 帧与 GPU 回读帧。
///
/// 规则（与任务口径一致）：
/// - CPU alpha = 0 且 GPU alpha = 0 → 双方背景，忽略；
/// - 一方覆盖另一方不覆盖 → 几何差异（记入 [`Diffs::geometry`]）；
/// - 双方覆盖 → alpha 必须为 255（否则记入 [`Diffs::alpha_violations`]），
///   并比较 RGB（alpha 恒 255，等价于比较 RGBA）。
pub fn compare(cpu: &CpuFrame, gpu: &[u8]) -> Diffs {
    assert_eq!(cpu.width, W, "CPU 帧宽必须与对比画布一致");
    assert_eq!(cpu.height, H, "CPU 帧高必须与对比画布一致");
    assert_eq!(
        cpu.pixels.len(),
        gpu.len(),
        "CPU 帧与 GPU 回读字节数必须一致"
    );

    let mut diffs = Diffs {
        total: (W * H) as usize,
        ..Diffs::default()
    };
    for index in 0..(W * H) as usize {
        let offset = index * 4;
        let cpu_px = [
            cpu.pixels[offset],
            cpu.pixels[offset + 1],
            cpu.pixels[offset + 2],
            cpu.pixels[offset + 3],
        ];
        let gpu_px = [
            gpu[offset],
            gpu[offset + 1],
            gpu[offset + 2],
            gpu[offset + 3],
        ];
        let x = index as u32 % W;
        let y = index as u32 / W;
        let cpu_covered = cpu_px[3] != 0;
        let gpu_covered = gpu_px[3] != 0;
        let diff = Diff {
            x,
            y,
            cpu: cpu_px,
            gpu: gpu_px,
        };

        if (cpu_covered && cpu_px[3] != 255) || (gpu_covered && gpu_px[3] != 255) {
            diffs.alpha_violations.push(diff);
            continue;
        }
        if cpu_covered != gpu_covered {
            diffs.geometry.push(diff);
            continue;
        }
        if !cpu_covered {
            continue;
        }
        diffs.covered += 1;
        let delta = (0..3)
            .map(|channel| cpu_px[channel].abs_diff(gpu_px[channel]))
            .max()
            .unwrap_or(0);
        if delta > COLOR_TOLERANCE {
            diffs.max_channel_delta = diffs.max_channel_delta.max(delta);
            diffs.color.push(diff);
        } else if delta > 0 {
            // 容差内的舍入差：只统计可见，不判失败（见 [`COLOR_TOLERANCE`]）。
            diffs.tolerated += 1;
            diffs.max_channel_delta = diffs.max_channel_delta.max(delta);
        }
    }
    diffs
}
