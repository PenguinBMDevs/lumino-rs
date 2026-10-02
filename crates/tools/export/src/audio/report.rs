//! 渲染进度与端口超限告警（自 `render_loops.rs` 拆出，保持单文件 < 400 行）

use lumino_midi_model::multi_port::MAX_PORTS;

use super::config::AudioRenderConfig;

/// B1 口径：端口超出产品上限（[`MAX_PORTS`]）时显式告警，列出超限轨道（截断 20 条）。
///
/// 不做整体拒绝、不静默丢音：事件会按 `effective_port` 折叠到端口 15 块发送；
/// 运行期实际折叠事件数由各渲染循环统计后在结束时汇总告警。
pub(super) fn warn_port_overflow(source: &str, track_ports: &[u8]) {
    let over: Vec<(usize, u8)> = track_ports
        .iter()
        .enumerate()
        .filter(|entry| *entry.1 >= MAX_PORTS)
        .map(|(idx, &port)| (idx, port))
        .collect();
    if over.is_empty() {
        return;
    }

    let mut detail: Vec<String> = over
        .iter()
        .take(20)
        .map(|(idx, port)| format!("轨{idx}:port{port}"))
        .collect();
    if over.len() > detail.len() {
        detail.push(format!("等 +{}", over.len() - detail.len()));
    }
    tracing::warn!(
        "[REND-002] {source}：{} 条轨道端口超出上限 {}（{}），已折叠到端口 {} 块（B1）；超出部分将共享该块通道状态",
        over.len(),
        MAX_PORTS,
        detail.join(", "),
        MAX_PORTS - 1
    );
}

/// REND-002：GPU 后端暂不支持多端口（#87），显式告警避免静默 16 通道折叠。
pub(super) fn warn_gpu_multi_port(midi_max_port: u8) {
    if midi_max_port != 0 {
        tracing::warn!(
            "[REND-002] GPU 导出暂不支持多端口（#87）：端口将被折叠到 16 通道，\
             端口间 CC/Program/声部可能串台；建议改用 CPU 后端"
        );
    }
}

/// 报告进度
pub(super) fn report_progress(
    config: &AudioRenderConfig,
    pct: f64,
    event_count: u64,
    note_count: u64,
    start_time: std::time::Instant,
    speed: Option<f64>,
) {
    let elapsed = start_time.elapsed();
    let speed_text = speed.map_or(String::new(), |s| format!(" | {s:.2}× 实时"));
    let msg = format!(
        "进度: {:.1}% | 事件: {} | 音符: {}{speed_text} | 耗时: {:.1}s",
        pct * 100.0,
        event_count,
        note_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = config.progress_callback {
        callback(msg, pct);
    } else {
        eprint!("\r{}  ", msg);
    }
}
