//! 实时事件处理吞吐量基准测试
//!
//! 注意：原 RealtimeEventEngine 已随 crate 重构移除（双实现清理）。
//! RealtimeSynth 依赖实际音频设备，不适合纯 CPU 吞吐量基准。
//! 此文件保留为编译存根，待后续接入基于 ChannelGroup 的独立基准。
//!
//! 如需测试事件吞吐量，建议直接使用 xsynth_core::channel_group::ChannelGroup
//! 构造独立测量循环。

fn main() {
    println!("Lumino Realtime 事件吞吐量基准测试");
    println!("=====================================");
    println!("(已迁移 — 见文件顶部注释)");
}
