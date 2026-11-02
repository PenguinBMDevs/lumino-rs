//! 洋葱皮渲染 — 全量流式分块 + 事件级增量上传（UI 线程 → WGPU 线程）
//!
//! 架构（2026-08-05 两轮优化：流式分块 + 事件级增量）：
//! - UI 线程：`stream_onion_skin_instances` 每帧由 `decide_action` 三态决策
//!   - `Full`：全量流式会话（首次 / 音轨进出洋葱皮 / mute / 调色板）——
//!     分块构建 NoteInstance（每块 ≤ 800 万实例 = 128 MB），逐块 send，
//!     每轨末尾强制 flush（空块 = 段表占位），最后 send `Done`
//!   - `Delta`：事件级增量（编辑洋葱皮音轨）——只构建被编辑音轨，send `TrackDelta`
//!   - `None`：无操作（只改当前/静音音轨 → 豁免）
//! - WGPU 线程：render loop 每帧 drain streaming channel
//!   - `Chunk` 流构建音轨段表（track_id → offset/len），`Done` 触发
//!     finish_streaming_upload（更新 cull info）
//!   - `TrackDelta`：等长 → 段内音符级替换；变长 → GPU 内部搬移后续段
//!     （无 CPU 镜像，黑乐谱单音轨海量音符场景不再全量重传）
//!
//! 渲染顺序：洋葱皮（不透明 alpha=1.0）→ 主音轨（不透明）→ UI
//! 深度测试：区域化位空间映射——预览 0.0 < 主音轨区 < 洋葱皮区，区内深度 =
//! 区起点 + 全局源索引（`chunk_start + 本地可见索引`），索引即深度序。
//! LessEqual 保证主音轨始终覆盖洋葱皮；重叠音符（同轨叠音 / 跨轨叠压）由
//! 稳定的全局索引决定先后，消除 GPU cull 并行重打包顺序随机导致的随机闪烁。
//! 编码方式：border_width 低 16 位 = 边框像素宽，高 16 位 = track_idx+1。
//!
//! # Q3 B：CPU 模式下 GPU 只保留当前轨
//!
//! `OnionSkinState::set_cpu_current_only(true)` 后（后端 = `OnionSkinBackend::Cpu`），
//! GPU 洋葱皮 buffer **只驻留当前轨**（非当前轨/静音轨由 CPU 叠加层负责），因此：
//! - 决策分支只产出 `Full` / `None`，**不再产出** `Delta` / `ViewState`；
//! - 全量会话走 `stream_onion_skin_full(..., Some(current_track))` 单轨会话；
//! - 非当前轨的 `pending_track_remove_ranges` 直接丢弃（GPU 无该轨数据）。
//!
//! 子模块组织（保持每个文件 < 400 行）：
//! - `state`：`OnionSkinState` 状态缓存（指纹、降级节流、CPU 单轨开关）
//! - `fingerprint`：指纹收集与静音集合哈希
//! - `decide`：`OnionSkinAction` 与三态决策（GPU / CPU 两条分支）
//! - `stream`：全量会话 / 轨布局 / 事件级增量上传
//! - `instance_stream`：`Host::stream_onion_skin_instances` 每帧调度
//! - `tests`：状态机决策测试

mod decide;
mod fingerprint;
mod instance_stream;
mod state;
mod stream;

pub(crate) use decide::OnionSkinAction;
pub(crate) use fingerprint::OnionSkinFingerprint;
pub(crate) use fingerprint::mute_fingerprint_of;
pub(crate) use state::OnionSkinState;

/// 洋葱皮描边宽度（固定 1 像素，与主音轨一致）
const ONION_SKIN_BORDER_WIDTH: u32 = 1;

/// 编码洋葱皮实例的 border_width：低 16 位 = 边框像素宽，
/// 高 16 位 = track_idx+1（VS 据此输出稳定深度，解决重叠音符闪烁）。
#[inline]
fn onion_border_width(track_idx: usize) -> u32 {
    ONION_SKIN_BORDER_WIDTH | (((track_idx as u32) + 1) << 16)
}

/// 流式上传分块大小（每块 ≤ 800 万实例 = 128 MB，大块减少传输次数）
const STREAMING_CHUNK_SIZE: usize = 8_000_000;

/// 并行实例构建的最小音符数（低于此规模顺序构建更快，避免线程调度开销）
const PARALLEL_BUILD_MIN: usize = 1_000_000;

/// 洋葱皮降级重试节流间隔（REND-019 PR-A：渲染线程被 GPU 拖住时通道持续满，
/// 避免每帧全量重建风暴）
const DEGRADE_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// 洋葱皮降级告警聚合间隔（REND-019 PR-A：日志节流，避免刷屏）
const DEGRADE_WARN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// 状态机三态决策测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "onion_skin/tests.rs"]
mod tests;

/// CPU 单轨模式（Q3 B）决策测试（独立文件，保持 `tests.rs` < 400 行）
#[cfg(test)]
#[path = "onion_skin/tests_cpu.rs"]
mod tests_cpu;
