//! 视频导出面板处理器

use crate::host::DialogResult;
use crate::message::{Message, VideoExportAction};
use crate::root::Root;
use crate::state::root_state::VideoExportOverlayState;
use std::str::FromStr;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_video_export(
        &self,
        root: &mut Root,
        action: VideoExportAction,
    ) -> Option<Message> {
        use VideoExportAction as V;
        match action {
            V::OpenPanel => {
                root.sidebar.video_export_visible = true;
                root.sidebar.route = crate::sidebar::Route::VideoExport;
            }
            V::ClosePanel => {
                root.sidebar.video_export_visible = false;
                root.sidebar.route = crate::sidebar::Route::Arrangement;
            }
            V::StartExport => {
                let st = &root.state.video_export_dialog;
                let document = root.midi.document.as_ref().map(std::sync::Arc::clone);
                // 先 clone 配置值，避免借用冲突
                let output_path = st.output_path.clone();
                let width = st.width;
                let height = st.height;
                let fps = st.fps;
                let container = st.container.clone();
                let codec = st.codec.clone();
                let backend = st.backend.clone();
                let quality = st.quality.clone();

                // 设置导出中状态
                root.state.video_export_dialog.overlay = VideoExportOverlayState::Exporting;
                root.state.video_export_dialog.progress = 0.0;
                root.state.video_export_dialog.status_message = "正在初始化...".to_string();
                root.state.video_export_dialog.current_frame = 0;
                root.state.video_export_dialog.total_frames = 0;
                root.state.video_export_dialog.render_fps = 0.0;
                root.state.video_export_dialog.cached_image_handle = None;

                let video_config = lumino_event::window::dialog::VideoExportConfig {
                    output_path,
                    width,
                    height,
                    fps,
                    ppq: root.editor.editor_state.view.ppq,
                    key_count: root.editor.editor_state.view.visible_key_count,
                    container: lumino_event::window::video::Container::from_str(&container)
                        .unwrap_or_default(),
                    codec: lumino_event::window::video::VideoCodec::from_str(&codec)
                        .unwrap_or_default(),
                    backend: lumino_event::window::video::EncoderBackend::from_str(
                        &backend,
                    )
                    .unwrap_or_default(),
                    quality: lumino_event::window::video::QualityPreset::from_str(&quality)
                        .unwrap_or_default(),
                    render_mode: root.state.video_export_dialog.render_mode,
                };
                let ev =
                    crate::event::window::Event::start_video_export(video_config, document);
                crate::event::emit(crate::event::Event::Window(ev));
            }
            V::CancelExport => {
                root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
                root.state.video_export_dialog.preview_frame = None;
                root.state.video_export_dialog.cached_image_handle = None;
                // 在对话框窗口中关闭窗口
                if root.state.is_dialog_window {
                    root.state.dialog_result = Some(DialogResult::Cancel);
                }
                // 通知 Runner 取消导出（关闭对话框 → 设置取消标志 → 后台线程退出）
                crate::event::emit(crate::event::Event::Window(
                    crate::event::window::Event::close_video_export_dialog(),
                ));
            }
            V::ForceFinish => {
                let st = &root.state.video_export_dialog;
                root.state.video_export_dialog.overlay =
                    VideoExportOverlayState::Completed {
                        total_frames: st.total_frames,
                        elapsed_secs: 0.0,
                        avg_fps: st.render_fps,
                    };
            }
            V::DismissOverlay => {
                root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
                root.state.video_export_dialog.preview_frame = None;
                root.state.video_export_dialog.cached_image_handle = None;
                // 在对话框窗口中关闭窗口
                if root.state.is_dialog_window {
                    root.state.dialog_result = Some(DialogResult::Cancel);
                }
            }
            V::ContainerChanged(v) => {
                root.state.video_export_dialog.container = v;
            }
            V::CodecChanged(v) => {
                root.state.video_export_dialog.codec = v;
            }
            V::BackendChanged(v) => {
                root.state.video_export_dialog.backend = v;
            }
            V::QualityChanged(v) => {
                root.state.video_export_dialog.quality = v;
            }
            V::WidthChanged(v) => {
                if v.chars().all(|c| c.is_ascii_digit())
                    && let Ok(w) = v.parse::<u32>()
                {
                    root.state.video_export_dialog.width = w;
                }
            }
            V::HeightChanged(v) => {
                if v.chars().all(|c| c.is_ascii_digit())
                    && let Ok(h) = v.parse::<u32>()
                {
                    root.state.video_export_dialog.height = h;
                }
            }
            V::FpsChanged(v) => {
                root.state.video_export_dialog.fps = v;
            }
            V::OutputPathChanged(v) => {
                root.state.video_export_dialog.output_path = v;
            }
            V::RenderModeChanged(v) => {
                tracing::info!("视频导出渲染模式切换: {}", v);
                root.state.video_export_dialog.render_mode = v;
            }
            V::BrowseOutput => {
                let st = &root.state.video_export_dialog;
                let ext = st.container.to_lowercase();
                let default_name = format!("output.{}", ext);
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .add_filter(&st.container, &[ext.as_str()])
                    .save_file()
                {
                    root.state.video_export_dialog.output_path =
                        path.to_string_lossy().to_string();
                }
            }
            V::UpdateProgress {
                message,
                progress,
                current_frame,
                total_frames,
                fps,
            } => {
                let st = &mut root.state.video_export_dialog;
                st.status_message = message;
                st.progress = progress;
                st.current_frame = current_frame;
                st.total_frames = total_frames;
                st.render_fps = fps;
            }
            V::ExportCompleted => {
                // 由 Runner 回调设置具体字段，此处不处理
            }
            V::ExportFailed(err) => {
                root.state.video_export_dialog.overlay =
                    VideoExportOverlayState::Error(err);
            }
            V::UpdatePreviewFrame { .. } => {
                // 由 Host 直接处理，此处不需要
            }
        }
        None
    }
}
