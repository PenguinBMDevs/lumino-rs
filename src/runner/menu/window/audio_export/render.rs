//! 后台渲染线程的启动与结果上报。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use lumino_core::{tr, trf};
use lumino_export::ExportResult;
use lumino_export::audio::config::AudioRenderConfig;
use lumino_midi_loader::MidiDocument;
use tokio::sync::mpsc::UnboundedSender;

use super::channels::ProgressMsg;

/// 启动后台渲染线程并立即上报初始进度。
pub(super) fn spawn_render_task(
    config: AudioRenderConfig,
    document: Option<Arc<MidiDocument>>,
    progress_tx: UnboundedSender<ProgressMsg>,
) {
    let output_path_display = config.output_path.display().to_string();
    let doc_clone = document.clone();
    let progress_tx_final = progress_tx.clone();
    let backend_for_log = config.backend;
    // 立即发送初始进度，避免 UI 长时间停在 0% 无反馈（GPU 初始化可能耗时）
    let _ = progress_tx.send((
        trf!("正在初始化 {backend} 渲染...", backend = backend_for_log),
        0.0,
        0,
        0.0,
        0.0,
    ));
    let _ = std::thread::Builder::new()
        .name("audio-render".into())
        .spawn(move || {
            let start = Instant::now();
            let render_result = render_export(&config, &doc_clone);
            report_render_result(
                render_result,
                Path::new(&output_path_display),
                progress_tx_final,
                start,
            );
        });
}

/// 按内存文档快照 / 文件模式执行渲染，返回渲染结果。
fn render_export(
    config: &AudioRenderConfig,
    document: &Option<Arc<MidiDocument>>,
) -> ExportResult<()> {
    match document {
        Some(doc) => {
            let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
            let total_ctrl = doc.control_events.len();
            tracing::info!(
                "内存文档快照: tracks={}, notes={}, ctrls={}, division={}, tempo_changes={}",
                doc.track_count,
                total_notes,
                total_ctrl,
                doc.division,
                doc.tempo_changes.len()
            );
            // 空文档回退到文件模式（macOS 上文档可能为空，避免直接报错）
            if total_notes == 0 && total_ctrl == 0 {
                tracing::warn!(
                    "内存文档为空（0 notes），回退到文件模式: {:?}",
                    config.midi_path
                );
                if config.midi_path.exists() {
                    lumino_export::audio::render_audio(config)
                } else {
                    Err(lumino_export::ExportError::AudioWrite(
                        tr!("内存文档为空且 MIDI 文件不存在，请先加载 MIDI 或检查 MIDI 路径")
                            .to_string(),
                    ))
                }
            } else {
                // Arc 快照不可变：克隆 owned 副本。
                // ChunkedList 为块级浅拷贝（O(块数) 指针拷贝），不复制音符数据。
                // tempo 已在文档内同步（审计 §三），无需再覆盖。
                let snapshot = (**doc).clone();
                lumino_export::audio::render_audio_from_document(config, &snapshot)
            }
        }
        None => lumino_export::audio::render_audio(config),
    }
}

/// 将渲染结果（成功 / 中止 / 失败）上报进度通道。
fn report_render_result(
    result: ExportResult<()>,
    output_path: &Path,
    progress_tx: UnboundedSender<ProgressMsg>,
    start: Instant,
) {
    match result {
        Ok(_) => {
            let elapsed = start.elapsed();
            tracing::info!(
                "音频导出完成: 耗时 {:.1}s, 输出={}",
                elapsed.as_secs_f64(),
                output_path.display(),
            );
            let _ = progress_tx.send((
                tr!("导出完成").to_string(),
                1.0,
                0,
                0.0,
                elapsed.as_secs_f64(),
            ));
        }
        Err(e) => {
            // 中止为可预期操作，删除残留文件后以“已中止”通知 UI
            if matches!(e, lumino_export::ExportError::Aborted) {
                let _ = std::fs::remove_file(output_path);
                tracing::info!("音频导出已中止，已清理: {}", output_path.display());
                let _ = progress_tx.send((tr!("已中止").to_string(), -1.0, 0, 0.0, 0.0));
            } else {
                tracing::error!("音频导出失败: {e}");
                let _ = progress_tx.send((format!("{e}"), -1.0, 0, 0.0, 0.0));
            }
        }
    }
}
