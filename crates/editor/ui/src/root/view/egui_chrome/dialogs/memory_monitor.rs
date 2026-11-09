//! 内存监控对话框：生产侧采样快照 → egui 模型，以及关闭动作 → `Message`。

use lumino_core::tr;
use lumino_diagnostics::memtrace::{AllocTag, Snapshot};
use lumino_ui_egui::chrome::{MemoryMetricModel, MemoryMonitorModel, MemoryRowModel};

use crate::Message;

impl crate::root::Root {
    /// 内存监控模型：标题 / 三行总指标 / 子系统分类 / 说明 / 关闭。
    ///
    /// 每次调用都重新采样（对话框以 ~300ms 心跳重绘，数字因此保持刷新）。
    pub(super) fn memory_monitor_model(&self) -> MemoryMonitorModel {
        let snapshot = Snapshot::capture();
        let rss_mb =
            lumino_diagnostics::memory_monitor::platform::get_current_rss() as f64 / 1_048_576.0;

        let metrics = vec![
            MemoryMetricModel {
                label: tr!("分配器追踪总内存").to_string(),
                value: format!("{:.1} MB", snapshot.total_mb()),
            },
            MemoryMetricModel {
                label: tr!("系统 RSS").to_string(),
                value: format!("{:.1} MB", rss_mb),
            },
            MemoryMetricModel {
                label: tr!("GPU 资源").to_string(),
                value: format!("{:.1} MB", snapshot.gpu_mb()),
            },
        ];

        let rows = AllocTag::ALL
            .iter()
            .filter(|tag| snapshot.get(**tag) > 0)
            .map(|tag| MemoryRowModel {
                name: tag.name().to_owned(),
                value: format!("{:.1} MB", snapshot.tracked_mb(*tag)),
            })
            .collect();

        MemoryMonitorModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("内存占用详情").to_string(),
            metrics,
            grid_title: tr!("按子系统分类").to_string(),
            rows,
            empty: tr!("（暂无子系统数据）").to_string(),
            note: tr!(
                "注：GPU 资源计数反映应用显式创建的 wgpu Texture/Buffer 大小；驱动层额外开销（swapchain、depth、pipeline cache 等）不纳入此项统计。"
            )
            .to_string(),
            close: tr!("关闭").to_string(),
        }
    }

    /// egui 内存监控动作 → 生产 `Message`（关闭窗口事件）。
    pub(crate) fn memory_monitor_action_message(
        &self,
        action: &lumino_ui_egui::memory_monitor::MemoryMonitorAction,
    ) -> Option<Message> {
        use lumino_ui_egui::memory_monitor::MemoryMonitorAction as Action;
        Some(match action {
            Action::Close => Message::Core(crate::event::Event::Window(
                crate::event::window::Event::close_memory_monitor_dialog(),
            )),
        })
    }
}
