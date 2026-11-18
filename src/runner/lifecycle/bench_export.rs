//! 诊断用自动视频导出（Tracy 基线 / 优化 A-B 回归）。
//!
//! MIDI 加载完成后按环境变量自动发起一次视频导出，正常启动零副作用。
//! 设计目标是「同一入口跑全部渲染风格」——复用真实导出管线（渲染线程 +
//! 合成 + 帧队列 + ffmpeg），不另起第二套渲染实现。
//!
//! 环境变量：
//! - `LUMINO_BENCH_EXPORT`：渲染模式（`waterfall` / `note_rectangle` / `miditrail` /
//!   `note_counter` / `data_curve` / `midi_console`，也接受 UI 显示名）；未设置则什么都不做。
//! - `LUMINO_BENCH_OUT`：输出路径，默认 `target/bench/<mode>.mp4`。
//! - `LUMINO_BENCH_FRAMES`：只渲染前 N 帧（默认不限制；基线建议 600）。
//! - `LUMINO_BENCH_DELAY_MS`：加载完成后延迟多少毫秒再开始（默认 3000，
//!   等洋葱皮全量上传完成，避免首帧一次性成本污染稳态）。
//! - `LUMINO_BENCH_W` / `LUMINO_BENCH_H` / `LUMINO_BENCH_FPS`：默认 1920 / 1080 / 60。
//! - `LUMINO_BENCH_MIDITRAIL_TOP=1`：MIDITrail 用 Top 视图。
//! - `LUMINO_BENCH_3D=1`：MIDITrail 3D 音符。
//! - `LUMINO_BENCH_MIDICONSOLE_CPU=1`：MidiConsole 用 CPU 栅格化后端。
//!
//! 配合 Tracy 抓取：
//! ```text
//! tracy-capture -o bench.tracy -s 120
//! lumino-rs.exe --test-time 60     # 同时设置上面的 LUMINO_BENCH_* 环境变量
//! ```

use std::sync::Arc;

use lumino_message::events::window::video::{
    Container, DataCurveConfig, EncoderBackend, MidiConsoleBackend, MidiConsoleConfig,
    MiditrailViewMode, NoteCounterConfig, QualityPreset, RenderMode, VideoCodec, VideoExportConfig,
};
use lumino_ui::event;

use crate::runner::RunnerInner;

/// 读取整型环境变量（非法值回退默认并告警）。
fn env_u32(name: &str, default: u32) -> u32 {
    match std::env::var(name) {
        Ok(raw) => raw.parse().unwrap_or_else(|_| {
            tracing::warn!("{name} 非法值 '{raw}'，回退 {default}");
            default
        }),
        Err(_) => default,
    }
}

/// 布尔开关：接受 `1/true/yes/on`（大小写不敏感）。
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

impl RunnerInner {
    /// MIDI 加载完成后调用：`LUMINO_BENCH_EXPORT` 存在时延迟发起一次导出。
    pub(crate) fn maybe_start_bench_export(&mut self) {
        let Ok(mode_raw) = std::env::var("LUMINO_BENCH_EXPORT") else {
            return;
        };
        let mode = match mode_raw.parse::<RenderMode>() {
            Ok(mode) => mode,
            Err(e) => {
                tracing::error!("LUMINO_BENCH_EXPORT 解析失败：{e}");
                return;
            }
        };

        // 文档从 UI 单一权威源取 Arc 浅 clone（与导出面板同一路径）
        let snapshot = {
            let host = self.window_state.window.ui();
            let root = host.root();
            root.editor.editor_state.data.document.as_ref().map(|doc| {
                (
                    Arc::new(doc.clone()),
                    root.editor.editor_state.view.ppq,
                    root.editor.editor_state.view.visible_key_count,
                )
            })
        };
        let Some((document, ppq, key_count)) = snapshot else {
            tracing::error!("LUMINO_BENCH_EXPORT：内存文档为空，无法发起导出");
            return;
        };

        let width = env_u32("LUMINO_BENCH_W", 1920);
        let height = env_u32("LUMINO_BENCH_H", 1080);
        let fps = env_u32("LUMINO_BENCH_FPS", 60);
        let out = std::env::var("LUMINO_BENCH_OUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(format!("target/bench/{}.mp4", mode.as_str()))
            });
        if let Some(parent) = out.parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut midi_console = MidiConsoleConfig::default();
        if env_flag("LUMINO_BENCH_MIDICONSOLE_CPU") {
            midi_console.render_backend = MidiConsoleBackend::Cpu;
        }

        let config = VideoExportConfig {
            output_path: out.to_string_lossy().into_owned(),
            midi_path: String::new(),
            width,
            height,
            fps,
            ppq: ppq.max(1),
            key_count,
            container: Container::Mp4,
            codec: VideoCodec::H264,
            backend: EncoderBackend::Software,
            quality: QualityPreset::Medium,
            render_mode: mode,
            waterfall_scroll_speed: 1.0,
            miditrail_view_mode: if env_flag("LUMINO_BENCH_MIDITRAIL_TOP") {
                MiditrailViewMode::Top
            } else {
                MiditrailViewMode::Normal
            },
            miditrail_normal_speed: 1.0,
            miditrail_top_speed: 1.0,
            miditrail_z_far: 7.5,
            miditrail_3d_notes: env_flag("LUMINO_BENCH_3D"),
            note_counter: NoteCounterConfig::default(),
            data_curve: DataCurveConfig::default(),
            midi_console,
        };

        let delay_ms = std::env::var("LUMINO_BENCH_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3000);
        tracing::info!(
            "Tracy 基准：{delay_ms}ms 后自动导出 mode={mode} {}x{}@{fps} -> {}",
            width,
            height,
            out.display()
        );

        // 延迟用独立线程：不阻塞加载/事件循环；事件缓冲是全局线程安全的。
        std::thread::Builder::new()
            .name("bench-export-delay".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                let ev = event::window::Event::start_video_export(config, Some(document));
                event::emit(event::Event::Window(ev));
            })
            .map(|_| ())
            .unwrap_or_else(|e| tracing::error!("Tracy 基准：延迟线程启动失败：{e}"));
    }
}
