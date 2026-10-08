//! 音符深度编码验证：区域化位空间契约（CPU 孪生）与「绘制顺序无关」的像素证据。
//!
//! 背景：cull.wgsl 每个 workgroup 由线程 0 抢占式 `atomicAdd` 输出槽位，可见实例的
//! **输出顺序**由 GPU 调度决定、帧间不稳定；而音符管线是 `LessEqual` +
//! `depth_write_enabled=true`，同深度「后画者胜」——赢家随可见缓冲顺序逐帧随机，
//! 表现为重叠区描边闪烁。修复手段是让深度只由「跨帧稳定的全局源索引」派生。
//!
//! 2026-09 黑乐谱加固：旧实现把索引注入基深度的 ulp 预算（主轨 629 万 /
//! 洋葱皮 209 万），实测黑乐谱（1936 万音符、单轨最高 494 万）大量音符落入饱和段
//! 共享同一深度 → 闪烁回归；且旧实现使用 **chunk 内局部索引**，多 chunk 时索引
//! 重置造成跨 chunk 深度别名（同深度平局在固定的 chunk 绘制顺序下虽稳定，但叠压
//! 关系与全局索引序相反）。现改为「区域化位空间映射」：
//!   预览 0.0 < 主音轨区 0x00800000 < 洋葱皮区 0x1F800000；
//!   区内 `bits = 区起点 + 全局索引`（`chunk_start + 本地可见索引`），每区
//!   5.2 亿槽位 —— 覆盖项目 2.9 亿目标，索引严格等价于深度序。
//!
//! 本模块给出两类可运行证据：
//!   1. 精度契约（CPU 孪生，无需 GPU）：区内严格单调（含实测黑乐谱规模与旧饱和
//!      边界）、区间不别名、不越远平面、chunk 折叠无别名、饱和有界；
//!   2. 像素证据（GPU）：同一场景、同一源数据，仅改变可见索引的**输出顺序**
//!      （复刻 cull 的随机槽位分配），连续 64 帧回读像素必须逐位一致；
//!      另有 chunk_start 折叠的构图验证（后画 chunk 的音符必须更靠后）。
//!
//! 已知边界：预览哨兵恒为 0.0（同批次多个哨兵仍共享深度，实际路径单哨兵）；
//! 索引超过 5.2 亿（超出项目 2.9 亿目标规模）后饱和到区顶——确定性但饱和段内
//! 仍可能平局，属记录在案的规模上限。
//!
//! 适配器能力门控：区域深度依赖 float32 深度语义；软件光栅器（CI ubuntu 的
//! Mesa 软渲染）会把亚 2^-24 深度压平（[0,1]→NDC 变换 / 定点深度实现），
//! 细粒度深度断言在该适配器上不可判定——两个像素测试先用
//! `adapter_resolves_region_depth` 探针判定能力，不满足时打印原因跳过；
//! 真实 GPU / WARP / Metal 路径全量断言，管线或 bind group 的真实错误仍以
//! wgpu 校验 panic 暴露（探针不会吞错）。

use super::NoteRenderer;
use super::types::{CameraUniform, CullUniform, DrawIndirectArgs};
use crate::constants::rendering::DEPTH_FORMAT;

// ═══ 1. 区域化位空间深度：CPU 孪生与精度契约 ════════════════════════════════
//
// 下列常量/函数必须与 4 个音符 shader（note / note_vertical / onion_note /
// onion_note_vertical）中的同名定义逐字对应——`shader_sources_share_depth_contract`
// 测试守住这份契约。

/// 主音轨区起点位模式（2^-126）——对应 shader `MAIN_DEPTH_REGION_BITS`
const MAIN_DEPTH_REGION_BITS: u32 = 0x0080_0000;
/// 洋葱皮区起点位模式——对应 shader `ONION_DEPTH_REGION_BITS`
const ONION_DEPTH_REGION_BITS: u32 = 0x1F80_0000;
/// 每区槽位数（5.2 亿）——对应 shader `DEPTH_REGION_SLOTS`
const DEPTH_REGION_SLOTS: u32 = 0x1F00_0000;

/// shader `region_depth` 的 CPU 孪生：区域起点 + 全局源索引（区顶饱和）。
fn region_depth(region_bits: u32, global_index: u32) -> f32 {
    f32::from_bits(region_bits + global_index.min(DEPTH_REGION_SLOTS - 1))
}

/// 主音轨区深度（主轨身份由 ViewState 判定，与 track_enc 无关）
fn main_region_depth(global_index: u32) -> f32 {
    region_depth(MAIN_DEPTH_REGION_BITS, global_index)
}

/// 洋葱皮区深度（区序 = 全局索引序 = 段表/轨道顺序）
fn onion_region_depth(global_index: u32) -> f32 {
    region_depth(ONION_DEPTH_REGION_BITS, global_index)
}

/// 全局索引 = chunk 基准 + 本地可见索引（对应 shader `chunk_info.chunk_start`）
fn global_index(chunk_start: u32, local_index: u32) -> u32 {
    chunk_start + local_index
}

/// 实测黑乐谱规模（`song for denise - piano fantasia`：1936 万音符 / 31 轨）与
/// 旧实现的全部饱和边界（209 万 / 629 万 / 838 万 / 1258 万）作为单调性采样点。
const SCALE_PROBE_INDICES: [u32; 12] = [
    0,
    1,
    4096,
    2_097_151,
    6_291_455,
    8_388_608,
    12_582_912,
    19_360_995,
    100_000_000,
    290_000_000,
    DEPTH_REGION_SLOTS - 2,
    DEPTH_REGION_SLOTS - 1,
];

mod contract;
mod gpu;
mod gpu_common;
