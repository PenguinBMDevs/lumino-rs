//! 后台文件对话框 — 对话框在独立线程打开，结果回传主线程消费
//!
//! 同步 `rfd::FileDialog` 会阻塞调用线程；UI 主线程直接调用会冻结窗口。
//! 这里统一把对话框挪到后台线程（rfd 内部处理 macOS 的主线程分发），
//! `Root` 在每帧消息路由时轮询结果并写回对应状态。
//!
//! 素材导入较特殊：对话框、格式校验与复制全部在后台线程完成，
//! 回传 [`MaterialImportOutcome`] 由主线程决定 toast 与重扫。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use super::Root;

/// 路径选择类对话框任务标识（决定结果写回哪个字段）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathDialogTask {
    /// 右侧栏「图片转 MIDI」源图片
    ImageToMidiSource,
    /// 音频导出：输出文件
    AudioExportOutput,
    /// 音频导出：MIDI 输入文件
    AudioExportMidi,
    /// 音频导出：音色库文件
    AudioExportSoundfont,
    /// 视频导出：计数器字体
    VideoCounterFont,
    /// 视频导出：计数器 CSV 输出
    VideoCounterCsv,
    /// 视频导出：数据曲线字体
    VideoDataCurveFont,
    /// 视频导出：输出文件
    VideoExportOutput,
    /// 视频导出：MIDI 输入文件
    VideoExportMidi,
}

/// 后台路径对话框结果接收端
pub(crate) type DialogRx = Receiver<Option<PathBuf>>;

/// 素材导入后台任务的回传结果
#[derive(Debug)]
pub(crate) enum MaterialImportOutcome {
    /// 用户取消选择
    Cancelled,
    /// 所选文件不是有效素材
    Invalid,
    /// 已复制到用户素材目录
    Imported,
    /// 复制失败（附错误描述）
    CopyFailed(String),
}

impl Root {
    /// 启动后台文件对话框任务（同一任务重复触发时丢弃旧任务）
    pub(crate) fn spawn_path_dialog(
        &mut self,
        task: PathDialogTask,
        open: impl FnOnce() -> Option<PathBuf> + Send + 'static,
    ) {
        self.pending_path_dialogs.retain(|(t, _)| *t != task);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(open());
        });
        self.pending_path_dialogs.push((task, rx));
    }

    /// 轮询后台路径对话框结果（每帧消息路由时调用）
    pub(crate) fn poll_path_dialogs(&mut self) {
        let mut finished: Vec<(usize, PathDialogTask, Option<PathBuf>)> = Vec::new();
        for (i, (task, rx)) in self.pending_path_dialogs.iter().enumerate() {
            match rx.try_recv() {
                Ok(path) => finished.push((i, *task, path)),
                Err(TryRecvError::Empty) => {}
                // 发送端已退出（后台线程异常）：按取消处理，清理槽位
                Err(TryRecvError::Disconnected) => finished.push((i, *task, None)),
            }
        }
        // 倒序移除，前面的索引不失效
        for (i, task, path) in finished.into_iter().rev() {
            self.pending_path_dialogs.remove(i);
            if let Some(path) = path {
                self.apply_path_dialog(task, path);
            }
        }
    }

    /// 将对话框结果写回对应状态字段
    fn apply_path_dialog(&mut self, task: PathDialogTask, path: PathBuf) {
        match task {
            PathDialogTask::ImageToMidiSource => {
                tracing::info!("已选择图片转 MIDI 源文件: {}", path.display());
                self.right_sidebar.set_selected_image_path(path);
            }
            PathDialogTask::AudioExportOutput => {
                self.state.audio_export_dialog.output_path = path.to_string_lossy().to_string();
            }
            PathDialogTask::AudioExportMidi => {
                self.state.audio_export_dialog.midi_path = path.to_string_lossy().to_string();
            }
            PathDialogTask::AudioExportSoundfont => {
                self.state.audio_export_dialog.soundfont_path = path.to_string_lossy().to_string();
            }
            PathDialogTask::VideoCounterFont => {
                self.state.video_export_dialog.counter_font_path =
                    path.to_string_lossy().to_string();
            }
            PathDialogTask::VideoCounterCsv => {
                self.state.video_export_dialog.counter_csv_output =
                    path.to_string_lossy().to_string();
            }
            PathDialogTask::VideoDataCurveFont => {
                self.state.video_export_dialog.dc_font_path = path.to_string_lossy().to_string();
            }
            PathDialogTask::VideoExportOutput => {
                self.state.video_export_dialog.output_path = path.to_string_lossy().to_string();
            }
            PathDialogTask::VideoExportMidi => {
                self.state.video_export_dialog.midi_path = path.to_string_lossy().to_string();
            }
        }
    }

    /// 启动素材导入后台任务（对话框、校验与复制均在后台线程完成）
    pub(crate) fn spawn_material_import(&mut self) {
        // 重复触发时丢弃旧任务（用户重新打开对话框）
        self.pending_material_import = None;
        let user_dir = crate::right_sidebar::user_materials_dir();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match rfd::FileDialog::new()
                .set_title("选择要导入的素材文件")
                .add_filter("Lumino 素材", &["lmmaterial"])
                .add_filter("所有文件", &["*"])
                .pick_file()
            {
                Some(path) => {
                    // 校验素材格式（从 metadata 判断是否为素材文件）
                    let valid_material = lumino_export::load_project(&path)
                        .map(|p| p.metadata.is_material_file())
                        .unwrap_or(false);
                    if !valid_material {
                        MaterialImportOutcome::Invalid
                    } else {
                        match crate::right_sidebar::copy_material_to_user_dir(&path, &user_dir) {
                            Ok(dest) => {
                                tracing::info!("素材已导入并复制到用户素材目录: {dest:?}");
                                MaterialImportOutcome::Imported
                            }
                            Err(e) => {
                                tracing::error!("素材复制失败: {e}");
                                MaterialImportOutcome::CopyFailed(e.to_string())
                            }
                        }
                    }
                }
                None => MaterialImportOutcome::Cancelled,
            };
            let _ = tx.send(outcome);
        });
        self.pending_material_import = Some(rx);
    }

    /// 轮询素材导入结果（每帧消息路由时调用）
    pub(crate) fn poll_material_import(&mut self) {
        let Some(rx) = self.pending_material_import.as_ref() else {
            return;
        };
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_material_import = None;
        match outcome {
            MaterialImportOutcome::Cancelled => {}
            MaterialImportOutcome::Invalid => {
                tracing::error!("导入失败：所选文件不是素材文件（.lmmaterial）");
                self.toast.push(
                    crate::toast::ToastLevel::Error,
                    "素材导入失败：不是有效的素材文件",
                );
            }
            MaterialImportOutcome::Imported => {
                self.toast
                    .push(crate::toast::ToastLevel::Success, "素材已导入");
                // 重新扫描列表
                self.start_material_scan();
            }
            MaterialImportOutcome::CopyFailed(e) => {
                tracing::error!("素材导入失败：{e}");
                self.toast.push(
                    crate::toast::ToastLevel::Error,
                    "素材导入失败：复制文件出错",
                );
            }
        }
    }
}
