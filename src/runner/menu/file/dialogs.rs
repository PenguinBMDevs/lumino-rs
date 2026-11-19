//! 文件菜单共用的系统消息对话框。
//!
//! 独立成文件：`load.rs` 负责加载链路，已接近 400 行红线；对话框只关心
//! `lumino-platform` 的展示细节，与加载逻辑无耦合。

#[cfg(not(test))]
use lumino_core::tr;

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 显示错误对话框
    pub(super) fn show_error_dialog(&self, message: &str) {
        #[cfg(not(test))]
        {
            let _ = lumino_platform::dialog::MessageDialog::new()
                .set_title(tr!("加载失败"))
                .set_description(message)
                .set_level(lumino_platform::dialog::MessageLevel::Error)
                .show();
        }
        #[cfg(test)]
        {
            let _ = message;
        }
        tracing::error!("{message}");
    }

    /// 显示信息对话框
    pub(super) fn show_info_dialog(&self, message: &str) {
        #[cfg(not(test))]
        {
            let _ = lumino_platform::dialog::MessageDialog::new()
                .set_title(tr!("提示"))
                .set_description(message)
                .set_level(lumino_platform::dialog::MessageLevel::Info)
                .show();
        }
        #[cfg(test)]
        {
            let _ = message;
        }
        tracing::info!("{message}");
    }
}
