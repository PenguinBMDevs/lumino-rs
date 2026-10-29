//! 启动时崩溃日志提示：扫描上次运行留下的 `crash_*.log`，归档并告知一次。

use lumino_core::{tr, trf};
use lumino_platform::dialog::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::runner::RunnerInner;
use crate::{logging, storage};

impl RunnerInner {
    /// 启动时若发现未归档的崩溃日志，移入 `logs/crash-archive/` 并提示一次。
    ///
    /// 归档本身即“已告知”标记：第二次启动扫描根目录为空，不会重复弹窗。
    pub(crate) fn notify_pending_crash_logs(&mut self) {
        let log_dir = storage::config_dir().join("logs");
        let pending = logging::pending_crash_logs(&log_dir);
        if pending.is_empty() {
            return;
        }

        let archive_dir = logging::archive_crash_logs(&log_dir, &pending).unwrap_or_else(|e| {
            tracing::warn!("崩溃日志归档失败: {e}");
            log_dir.clone()
        });

        let open_label = tr!("打开日志目录").to_string();
        let close_label = tr!("关闭").to_string();
        let result = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(tr!("上次运行发生崩溃"))
            .set_description(trf!(
                "发现 {count} 份崩溃日志，已归档到：\n\n{path}\n\n可打开日志目录查看详情。",
                count = pending.len(),
                path = archive_dir.display()
            ))
            .set_buttons(MessageButtons::OkCancelCustom(
                open_label.clone(),
                close_label,
            ))
            .show();

        if matches!(result, MessageDialogResult::Custom(label) if label == open_label)
            && let Err(e) = logging::open_log_dir(&archive_dir)
        {
            tracing::warn!("打开崩溃日志目录失败 {:?}: {e}", archive_dir);
        }
    }
}
