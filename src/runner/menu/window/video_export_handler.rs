//! 视频导出窗口事件处理：在后台线程执行逐帧渲染 + FFmpeg 编码，进度通过通道回传主线程。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

use lumino_core::{tr, trf};
use lumino_export::video::{
    Container, EncoderBackend, QualityPreset, VideoCodec, VideoExportConfig,
};
use lumino_gfx::TextureFormat;
use lumino_gfx::render_thread::RenderCommand;
use lumino_message::events::window::video::VideoExportConfig as EventVideoExportConfig;
use lumino_message::events::window::video::{
    Container as EventContainer, DataCurveConfig, EncoderBackend as EventEncoderBackend,
    MidiConsoleConfig, MiditrailViewMode, NoteCounterConfig, QualityPreset as EventQualityPreset,
    RenderMode, VideoCodec as EventVideoCodec,
};
use tokio::sync::mpsc::UnboundedSender;

use memory_task::{RunVideoExportTaskInput, run_video_export_task};

mod commands;
mod composite;
mod encoder_thread;
mod frame;
mod memory_task;
mod pipeline;
// 流式视频导出（`streaming_task` + `video_export::streaming`）已按审计 §八 删除：
// 该链路 0 调用者（`run_streaming_video_export_task` 全仓无调用），且它自带第二套
// MIDI 解析 + 磁盘 `NoteRecord` 镜像（审计 §6.2 / 表格第 18 行）。
// 需要重做时：实现本体在 git 历史（删除提交见报告 §十九）与其 `NoteRecord` 磁盘缓存
// 设计；**不要**再起第二套音符镜像——直接消费 `MidiDocument`（唯一权威源）。

/// 进度消息通道发送端（message, progress, total_frames, render_fps, elapsed_secs）。
type ProgressTx = UnboundedSender<(String, f64, u64, f64, f64)>;
/// 预览帧通道发送端（RGBA 像素数据 + 宽高）。
type PreviewTx = UnboundedSender<(Vec<u8>, u32, u32)>;

/// MIDITrail 相关参数（从事件配置归组，避免启动参数结构体字段超上限）。
struct MiditrailParams {
    z_far: f32,
    view_mode: MiditrailViewMode,
    normal_speed: f32,
    top_speed: f32,
    notes_3d: bool,
}

/// 三种模式专属渲染配置（由 `render_mode` 择一生效）。
struct RenderConfigs {
    note_counter: NoteCounterConfig,
    data_curve: DataCurveConfig,
    midi_console: MidiConsoleConfig,
}

/// 事件层枚举 → 导出层枚举的 1:1 映射结果。
struct MappedEnums {
    container: Container,
    codec: VideoCodec,
    backend: EncoderBackend,
    quality: QualityPreset,
}

/// 入口解构后交给启动阶段的参数集合。
struct LaunchParams {
    output_path: String,
    midi_path: String,
    width: u32,
    height: u32,
    fps: u32,
    ppq: u16,
    key_count: u16,
    container: EventContainer,
    codec: EventVideoCodec,
    backend: EventEncoderBackend,
    quality: EventQualityPreset,
    render_mode: RenderMode,
    waterfall_scroll_speed: f32,
    miditrail: MiditrailParams,
    render_configs: RenderConfigs,
    document: Option<Arc<lumino_midi_loader::MidiDocument>>,
}

/// 后台线程启动参数：由启动阶段组装后整体移入线程闭包。
struct SpawnParams {
    config: VideoExportConfig,
    cmd_sender: Sender<RenderCommand>,
    progress_tx: ProgressTx,
    preview_tx: PreviewTx,
    document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    midi_path: String,
    ppq: u32,
    fps_f64: f64,
    key_count: u16,
    width: u32,
    height: u32,
    cancel_flag: Arc<AtomicBool>,
    tdr_cancel_flag: Arc<AtomicBool>,
    surface_pix_fmt: &'static str,
    render_mode: RenderMode,
    render_configs: RenderConfigs,
    waterfall_scroll_speed: f32,
    miditrail: MiditrailParams,
}

/// 线程内由渲染模式与表面格式派生的参数。
struct ThreadRenderParams {
    input_pix_fmt: &'static str,
    is_cpu_renderer: bool,
    is_gpu_compute_style: bool,
    counter_config: Option<super::video_export::CounterRenderConfig>,
    data_curve_config: Option<super::video_export::DataCurveRenderConfig>,
    midi_console_config: Option<super::video_export::MidiConsoleRenderConfig>,
}

impl RunnerInner {
    pub(crate) fn handle_start_video_export(
        &mut self,
        config: Box<EventVideoExportConfig>,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) {
        // CPU 洋葱皮后端：**最终兜底**。独立视频导出对话框的 Root 取的是创建时的
        // `UiConfig`（`hires.backend` 可能滞后于运行时真源），其入口闸只能尽力而为；
        // 主窗口后端才是唯一真源，因此在这里再拦一次——不置导出状态、不启后台线程。
        if self.reject_video_export_on_cpu() {
            return;
        }

        if self.reject_export_already_running() {
            return;
        }

        let EventVideoExportConfig {
            output_path,
            midi_path,
            width,
            height,
            fps,
            ppq,
            key_count,
            container,
            codec,
            backend,
            quality,
            render_mode,
            waterfall_scroll_speed,
            miditrail_z_far,
            miditrail_view_mode,
            miditrail_normal_speed,
            miditrail_top_speed,
            miditrail_3d_notes,
            note_counter,
            data_curve,
            midi_console,
        } = *config;

        if self.validate_export_output_path(&output_path) {
            return;
        }

        self.launch_video_export(LaunchParams {
            output_path,
            midi_path,
            width,
            height,
            fps,
            ppq,
            key_count,
            container,
            codec,
            backend,
            quality,
            render_mode,
            waterfall_scroll_speed,
            miditrail: MiditrailParams {
                z_far: miditrail_z_far,
                view_mode: miditrail_view_mode,
                normal_speed: miditrail_normal_speed,
                top_speed: miditrail_top_speed,
                notes_3d: miditrail_3d_notes,
            },
            render_configs: RenderConfigs {
                note_counter,
                data_curve,
                midi_console,
            },
            document,
        });
    }

    /// 导出可重入守卫：已有导出进行中时上报状态并拒绝。
    fn reject_export_already_running(&mut self) -> bool {
        // DEBT-07 #124：导出可重入守卫（打开对话框之前拦截）——已有音频导出或
        // 视频导出进行中时拒绝，避免进度/预览通道与取消标志被覆盖。
        if self.window_state.audio_export_control.is_some()
            || self
                .window_state
                .video_export_running
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            tracing::warn!("已有导出任务进行中，忽略新的视频导出请求");
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(tr!("已有导出任务进行中，请等待完成").to_string()));
            return true;
        }
        false
    }

    /// 输出路径兜底校验：空路径在开窗、置运行标志、起线程之前拒绝。
    fn validate_export_output_path(&mut self, output_path: &str) -> bool {
        // 输出路径是硬性前提：空串会让 ffmpeg 以 `Error opening output file .`
        // （code -22）退出，而导出线程已经白跑一轮、stderr 刷屏。UI 面板入口已
        // 拦截（Toast + 不置导出态）；这里兜底所有事件来源（菜单/快捷键/旧配置
        // 回放），与 CPU 守卫同源——在开窗、置运行标志、起线程之前拒绝。
        if output_path.trim().is_empty() {
            tracing::warn!("视频导出失败：输出路径为空（用户未选择导出位置）");
            self.window_state.window.ui_mut().root_mut().push_toast(
                lumino_ui::toast::ToastLevel::Warning,
                tr!("请先选择视频输出路径"),
            );
            return true;
        }
        false
    }

    /// 启动阶段：枚举映射 → 开窗取通道 → 登记状态/通道 → 组装输入 → 起后台线程。
    fn launch_video_export(&mut self, p: LaunchParams) {
        // 事件层枚举 → 导出层枚举（总映射，无字符串解析、无静默降级）
        let MappedEnums {
            container,
            codec,
            backend,
            quality,
        } = map_event_enums(p.container, p.codec, p.backend, p.quality);

        tracing::info!(
            "开始视频导出: {}x{} @ {}fps, 容器={:?}, 编解码器={:?}",
            p.width,
            p.height,
            p.fps,
            container,
            codec
        );

        let Some((cmd_sender, surface_pix_fmt)) = self.open_dialog_and_get_sender() else {
            return;
        };

        // 守卫已通过：登记运行标志（线程内 RAII 保证任何退出路径复位）
        self.window_state
            .video_export_running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // 创建进度通道（复用音频导出的进度通道机制）
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        self.window_state.export_progress_rx = Some(progress_rx);

        // 创建预览帧通道
        let (preview_tx, preview_rx) = tokio::sync::mpsc::unbounded_channel();
        self.window_state.video_preview_rx = Some(preview_rx);

        // 构建 VideoExportConfig
        let config = VideoExportConfig {
            width: p.width,
            height: p.height,
            fps: p.fps as f64,
            container,
            codec,
            backend,
            output_path: std::path::PathBuf::from(&p.output_path),
            quality,
        };

        let ppq = p.ppq.max(1) as u32;
        let fps_f64 = p.fps as f64;

        // 创建取消标志
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.window_state.video_export_cancel = Arc::clone(&cancel_flag);
        // 每次导出新建 TDR 归因标志（重连流水线置位；收尾按失败上报，R5-b）
        let tdr_cancel_flag = Arc::new(AtomicBool::new(false));
        self.window_state.video_export_tdr_cancel = Arc::clone(&tdr_cancel_flag);

        self.spawn_video_export_thread(SpawnParams {
            config,
            cmd_sender,
            progress_tx,
            preview_tx,
            document: p.document,
            midi_path: p.midi_path,
            ppq,
            fps_f64,
            key_count: p.key_count,
            width: p.width,
            height: p.height,
            cancel_flag,
            tdr_cancel_flag,
            surface_pix_fmt,
            render_mode: p.render_mode,
            render_configs: p.render_configs,
            waterfall_scroll_speed: p.waterfall_scroll_speed,
            miditrail: p.miditrail,
        });
    }

    /// 打开导出对话框并取得渲染命令通道；通道缺失时上报失败并返回 None。
    fn open_dialog_and_get_sender(&mut self) -> Option<(Sender<RenderCommand>, &'static str)> {
        // 打开视频导出对话框（进度显示）
        self.window_state
            .dialog_manager
            .open_dialog(DialogType::VideoExport);

        // 获取渲染线程命令发送端与表面纹理格式（非 GPU compute 样式使用）
        let main_ui = self.window_state.window.ui();
        let cmd_sender = main_ui.render_command_sender();
        let surface_pix_fmt = match main_ui.texture_format() {
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => "bgra",
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => "rgba",
            _ => "bgra",
        };

        let Some(cmd_sender) = cmd_sender else {
            tracing::error!("视频导出失败：渲染线程未启动");
            let main_ui = self.window_state.window.ui_mut();
            main_ui.set_video_export_failed(tr!("渲染线程未启动").to_string());
            return None;
        };
        Some((cmd_sender, surface_pix_fmt))
    }

    /// 启动后台渲染线程；spawn 失败时复位运行标志并通知对话框。
    fn spawn_video_export_thread(&mut self, p: SpawnParams) {
        // 后台线程：逐帧渲染 + FFmpeg 编码
        let running = Arc::clone(&self.window_state.video_export_running);
        let spawn_result = std::thread::Builder::new()
            .name("video-render".into())
            .spawn(move || {
                // DEBT-07 #124：任何退出路径（完成/失败/取消）都复位运行标志
                let _running_guard = VideoExportRunningGuard(running);
                run_video_export_in_thread(p);
            });
        if let Err(e) = spawn_result {
            // 线程未启动：复位标志并通知对话框失败（旧实现静默吞掉 spawn 失败）
            self.window_state
                .video_export_running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            tracing::error!("视频导出线程启动失败: {e}");
            self.window_state
                .dialog_manager
                .forward_video_export_failed(trf!("导出线程启动失败: {e}", e = e));
            self.window_state
                .dialog_manager
                .mark_dialog_for_close(DialogType::VideoExport);
        }
    }
}

/// 事件层枚举 → 导出层枚举（总映射，无字符串解析、无静默降级）。
fn map_event_enums(
    container: EventContainer,
    codec: EventVideoCodec,
    backend: EventEncoderBackend,
    quality: EventQualityPreset,
) -> MappedEnums {
    let container = match container {
        EventContainer::Mp4 => Container::Mp4,
        EventContainer::Mov => Container::Mov,
        EventContainer::Mkv => Container::Mkv,
        EventContainer::Avi => Container::Avi,
    };
    let codec = match codec {
        EventVideoCodec::H264 => VideoCodec::H264,
        EventVideoCodec::H265 => VideoCodec::H265,
        EventVideoCodec::ProRes => VideoCodec::ProRes,
        EventVideoCodec::Vp9 => VideoCodec::Vp9,
        EventVideoCodec::Av1 => VideoCodec::Av1,
    };
    let backend = match backend {
        EventEncoderBackend::Software => EncoderBackend::Software,
        EventEncoderBackend::VideoToolbox => EncoderBackend::VideoToolbox,
        EventEncoderBackend::Nvenc => EncoderBackend::Nvenc,
        EventEncoderBackend::Amf => EncoderBackend::Amf,
        EventEncoderBackend::Qsv => EncoderBackend::Qsv,
        EventEncoderBackend::Vaapi => EncoderBackend::Vaapi,
    };
    let quality = match quality {
        EventQualityPreset::High => QualityPreset::High,
        EventQualityPreset::Medium => QualityPreset::Medium,
        EventQualityPreset::Low => QualityPreset::Low,
    };
    MappedEnums {
        container,
        codec,
        backend,
        quality,
    }
}

/// 由渲染模式与表面格式派生线程内的像素格式、渲染器类别与模式专属配置。
fn resolve_render_params(
    render_mode: RenderMode,
    surface_pix_fmt: &'static str,
    configs: RenderConfigs,
) -> ThreadRenderParams {
    let is_gpu_compute_style = matches!(
        render_mode,
        lumino_message::events::window::video::RenderMode::Waterfall
            | lumino_message::events::window::video::RenderMode::MIDITrail
    );
    // 计数器模式：CPU 端渲染（无卷帘/键盘/标尺），帧数据为 BGRA 直出。
    let is_cpu_renderer = matches!(
        render_mode,
        lumino_message::events::window::video::RenderMode::NoteCounter
            | lumino_message::events::window::video::RenderMode::DataCurve
            | lumino_message::events::window::video::RenderMode::MidiConsole
    );
    // GPU compute / 3D 渲染输出为 rgba8unorm storage texture，
    // 因此编码器输入像素格式必须为 "rgba"；
    // 计数器 CPU 渲染直出 BGRA → "bgra"；
    // 其余（NoteRectangle）使用 UI 表面纹理，按表面格式选择。
    let input_pix_fmt = if is_gpu_compute_style {
        "rgba"
    } else if is_cpu_renderer {
        "bgra"
    } else {
        surface_pix_fmt
    };
    // 计数器渲染配置（仅 NoteCounter 模式有效）
    let counter_config = if matches!(
        render_mode,
        lumino_message::events::window::video::RenderMode::NoteCounter
    ) {
        Some(super::video_export::CounterRenderConfig::from(
            &configs.note_counter,
        ))
    } else {
        None
    };
    // 数据曲线渲染配置（仅 DataCurve 模式有效）
    let data_curve_config = if matches!(
        render_mode,
        lumino_message::events::window::video::RenderMode::DataCurve
    ) {
        Some(super::video_export::DataCurveRenderConfig::from(
            &configs.data_curve,
        ))
    } else {
        None
    };
    // MidiConsole 风格渲染配置（仅 MidiConsole 模式有效）
    let midi_console_config = if matches!(
        render_mode,
        lumino_message::events::window::video::RenderMode::MidiConsole
    ) {
        Some(super::video_export::MidiConsoleRenderConfig::from(
            &configs.midi_console,
        ))
    } else {
        None
    };
    ThreadRenderParams {
        input_pix_fmt,
        is_cpu_renderer,
        is_gpu_compute_style,
        counter_config,
        data_curve_config,
        midi_console_config,
    }
}

/// 后台线程主体：派生渲染参数，按数据源分支执行任务或上报错误。
fn run_video_export_in_thread(p: SpawnParams) {
    let ThreadRenderParams {
        input_pix_fmt,
        is_cpu_renderer,
        is_gpu_compute_style,
        counter_config,
        data_curve_config,
        midi_console_config,
    } = resolve_render_params(p.render_mode, p.surface_pix_fmt, p.render_configs);

    if let Some(document) = p.document {
        // Arc 快照不可变：克隆 owned 副本。
        // ChunkedList 为块级浅拷贝（O(块数) 指针拷贝），不复制音符数据。
        // tempo 无需覆盖（2026-10-20 审计 §三）：`EditorData` 的 tempo 写入
        // 入口都会 `sync_tempo_to_document`，快照里的 tempo_changes 已是
        // 编辑后的值；原 `snapshot.tempo_changes = editor_tempos` 已删除。
        let snapshot = (*document).clone();
        run_video_export_task(RunVideoExportTaskInput {
            config: p.config,
            cmd_sender: p.cmd_sender,
            progress_tx: p.progress_tx,
            preview_tx: p.preview_tx,
            document: Arc::new(snapshot),
            ppq: p.ppq,
            fps_f64: p.fps_f64,
            key_count: p.key_count,
            width: p.width,
            height: p.height,
            cancel_flag: p.cancel_flag,
            tdr_cancel_flag: p.tdr_cancel_flag,
            input_pix_fmt,
            is_cpu_renderer,
            is_gpu_compute_style,
            waterfall_scroll_speed: p.waterfall_scroll_speed,
            miditrail_z_far: p.miditrail.z_far,
            miditrail_view_mode: p.miditrail.view_mode,
            miditrail_normal_speed: p.miditrail.normal_speed,
            miditrail_top_speed: p.miditrail.top_speed,
            miditrail_3d_notes: p.miditrail.notes_3d,
            render_mode: p.render_mode,
            counter_config,
            data_curve_config,
            midi_console_config,
        });
    } else if !p.midi_path.is_empty() {
        // TODO(数据源统一): 流式读取模式视频导出已暂时禁用，后续基于
        // 单一权威数据源（GpuNoteBuffer / note_instances）重做流式路径。
        tracing::error!("视频导出失败：流式读取模式已暂时禁用，请先加载工程");
        send_export_error(
            &p.progress_tx,
            tr!("导出失败：流式读取模式已暂时禁用，请先加载工程"),
        );
    } else {
        tracing::error!("视频导出失败：无 MidiDocument 且未指定 MIDI 路径");
        send_export_error(&p.progress_tx, tr!("导出失败：无 MIDI 数据"));
    }
}

/// 视频导出运行标志的 RAII 守卫（DEBT-07 #124）：线程任何退出路径都会复位。
struct VideoExportRunningGuard(Arc<AtomicBool>);

impl Drop for VideoExportRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 发送导出失败进度消息（progress=-1 表示失败，UI 据此弹出错误）。
///
/// 收敛各处重复的 5 元组 `("导出失败: ..", -1.0, 0, 0.0, 0.0)` 发送。
fn send_export_error(
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    message: impl Into<String>,
) {
    let _ = progress_tx.send((message.into(), -1.0, 0, 0.0, 0.0));
}
