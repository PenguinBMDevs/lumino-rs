//! 导出控制检查与渲染进度回调

use super::*;

pub(super) fn check_control(config: &AudioRenderConfig) -> ExportResult<()> {
    if let Some(ctrl) = &config.control {
        ctrl.wait_if_paused();
        ctrl.check_abort()?;
    }
    Ok(())
}

/// 把 GPU 引擎的预载/渲染进度接到导出进度回调（#35）。
///
/// 映射区间：预载 0.10→0.20、渲染 0.20→0.85；节流为进度每前进 0.1%
/// 才转发一次，避免黑 MIDI 每块刷 UI。
pub(super) fn attach_render_progress(
    synth: &mut lumino_gpu_synth::GpuSynth,
    config: &AudioRenderConfig,
) {
    let Some(cb) = config.progress_callback.clone() else {
        return;
    };
    let sample_rate = config.sample_rate as f64;
    let started = std::time::Instant::now();
    let last_permille = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let speed = std::sync::Arc::new(std::sync::Mutex::new(ExportSpeedMeter::new(
        DEFAULT_SPEED_WINDOW_SECS,
    )));
    synth.set_render_progress(Some(std::sync::Arc::new(
        move |p: lumino_gpu_synth::RenderProgress| {
            let elapsed = started.elapsed().as_secs_f64();
            // 倍速 = 已渲染音频秒 / 墙钟秒，取 3s 窗口平均（避免瞬时抖动）。
            let speed_now = match p {
                lumino_gpu_synth::RenderProgress::Render { done, .. } => {
                    let mut meter = speed
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    meter.record(elapsed, done as f64 / sample_rate);
                    meter.speed()
                }
                _ => None,
            };
            let (pct, msg) = render_progress_message(p, elapsed, speed_now);
            let permille = (pct.clamp(0.0, 1.0) * 1000.0) as u64;
            if permille > last_permille.load(std::sync::atomic::Ordering::Relaxed) {
                last_permille.store(permille, std::sync::atomic::Ordering::Relaxed);
                cb(msg, pct);
            }
        },
    )));
}

/// 引擎进度事件 -> 导出进度百分比与展示文案。
///
/// 进度总量为 0 时按 0% 处理（避免除零），文案与 CPU 路径的
/// `进度: xx.x% | ... | 耗时 xx.xs` 风格保持一致。
pub(super) fn render_progress_message(
    progress: lumino_gpu_synth::RenderProgress,
    elapsed_secs: f64,
    speed: Option<f64>,
) -> (f64, String) {
    match progress {
        lumino_gpu_synth::RenderProgress::Prewarm { done, total } => {
            let frac = if total == 0 {
                0.0
            } else {
                done as f64 / total as f64
            };
            let pct = 0.10 + 0.10 * frac;
            (
                pct,
                format!(
                    "进度: {:.1}% | 预载音色库 {done}/{total} | 耗时 {elapsed_secs:.1}s",
                    pct * 100.0
                ),
            )
        }
        lumino_gpu_synth::RenderProgress::Render { done, total } => {
            let frac = if total == 0 {
                0.0
            } else {
                (done as f64 / total as f64).clamp(0.0, 1.0)
            };
            let pct = 0.20 + 0.65 * frac;
            let speed_text = speed.map_or(String::new(), |s| format!(" | {s:.2}× 实时"));
            (
                pct,
                format!(
                    "进度: {:.1}% | 帧 {done}/{total}{speed_text} | 耗时 {elapsed_secs:.1}s",
                    pct * 100.0
                ),
            )
        }
    }
}
