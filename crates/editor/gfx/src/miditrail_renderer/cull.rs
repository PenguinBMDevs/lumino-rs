//! Miditrail 导出 cull：首帧全量常驻 + 每帧 GPU 窗口提取 + 回读 legacy 渲染。
//!
//! 背景：24M 级文档下 UI 每帧 collect/sort/pack（~20ms）是导出主瓶颈；全量常驻
//! 曾因“390MB 显存＋390MB 镜像＋每帧两次全扫 100ms”被否决。本路径只保留必要
//! 部分：全量 GPU 常驻一次上传（310MB，与钢琴模式首帧全量同量级，无 CPU 镜像），
//! 每帧 cull 内核按桶分区提取有序窗口（`bucket_cull.wgsl`，与 UI 同谓词），
//! 回读 compact（V×16B）后走未经修改的 legacy `render_from_instances`——
//! 像素与现状逐位一致（集合等价 harness 保证），视觉 veto 不触发。
//!
//! 两次提交：COUNT 自有提交（含 1KB 回读，`ResidentCull` 内部）；FILL + compact
//! 回读自有提交（legacy 渲染需 CPU 切片，读回后新 encoder 渲染）。回读量 V×16B
//!（36 万可见约 6MB），相对省掉的 UI 排序可忽略，打点量化。
//!
//! 两条出口：
//! - `cull_window`：Top 视图/回退用（回读 V×16B → legacy 量化渲染）；
//! - `cull_prepare`：Normal driven 用（零音符回读；COUNT 顺带聚合活跃键/光晕
//!   并回读 1KB，FILL 直写 compact 供 driven 顶点管线直绑）。

mod driven;
mod lifecycle;
mod window;

use super::instances::ActiveKeys;
use crate::KEY_BUCKETS;

/// cull 窗口分段耗时（随 `cull_window`/`cull_prepare` 返回；打点拆分用）。
#[derive(Debug, Default, Clone, Copy)]
pub struct CullTiming {
    /// COUNT 内核 + 提交 + 回读同步（`cull_prepare` 含活跃键 1KB 回读）。
    pub count_us: u64,
    /// `cull_window`：FILL + compact 回读提交 + V×16B 按需映射拷贝；
    /// `cull_prepare`：FILL 调度（无 compact 回读）。
    pub fill_readback_us: u64,
}

/// `cull_prepare` 产物：driven 渲染所需的窗口规模 + 活跃键聚合。
#[derive(Debug, Clone)]
pub struct CullPrepared {
    /// 窗口音符总数（driven `draw_indexed` 实例数）。
    pub total: usize,
    /// 活跃键聚合：`[0,128)` 键色（0 = 未按下），`[128,256)` 光晕系数 bitcast。
    pub active: [u32; KEY_BUCKETS],
    /// 分段耗时（口径见 `CullTiming`）。
    pub timing: CullTiming,
}

/// 将 GPU 活跃键聚合解码为 `(ActiveKeys, aura_sizes)`。
///
/// 布局：`[0,128)` 键色（0 = 未按下），`[128,256)` 光晕系数 f32 bitcast。
/// 与 `compute_active_and_aura_for_compact` 的 CPU 语义逐位对齐；keys ≥ key_count
/// 的尾部不采信（防上一帧残留）。
pub(super) fn decode_active_for_gpu(
    active: &[u32; KEY_BUCKETS],
    key_count: usize,
) -> (ActiveKeys, [f32; 128]) {
    let limit = key_count.min(128);
    let mut keys = ActiveKeys {
        pressed: [false; 128],
        colors: [0u32; 128],
    };
    let mut aura_sizes = [0.0f32; 128];
    for k in 0..limit {
        let c = active[k];
        if c != 0 {
            keys.pressed[k] = true;
            keys.colors[k] = c;
        }
        aura_sizes[k] = f32::from_bits(active[128 + k]);
    }
    (keys, aura_sizes)
}
