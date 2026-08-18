//! Root 状态同步方法
//!
//! 云存储 UI 状态同步（主窗口 ↔ 对话框）、素材扫描请求、
//! 编辑器引用获取与非阻塞播放帧拉取。

use crate::editor;
use crate::root::Root;

impl Root {
    /// 从另一个 Root 同步云存储 UI 状态（用于对话框窗口同步主窗口状态）。
    ///
    /// 云存储的**唯一数据源**是主窗口 Root：连接快照/目录列表/结果提示均由
    /// runner 注入主窗口；对话框（连接/浏览/提醒/设置）打开时及状态变化后
    /// 通过本方法拉取最新快照，保证"已连接设备"在设置面板与文件浏览器中可见。
    pub fn sync_cloud_state_from(&mut self, other: &Root) {
        self.cloud = other.cloud.clone();
        // 设置面板云管理页（连接列表 + 断连提醒标志）
        self.settings.cloud.connections = other.settings.cloud.connections.clone();
        self.settings.cloud.alert = other.settings.cloud.alert.clone();
    }

    /// 从另一个 Root 同步云存储**共享快照**（运行期广播用）。
    ///
    /// 与 `sync_cloud_state_from`（完整拷贝，对话框首次打开时回显）不同，
    /// 本方法**排除连接表单字段**（协议/名称/地址/端口/用户名/密码/连接中/
    /// 错误）与本地编辑字段（新建文件夹输入），避免用户正在输入时被后台
    /// 状态广播覆盖。浏览数据（设备/导航/列表）由事件回传保持主窗口与
    /// 对话框一致后同步，保存模式切换目录不会弹回根目录。
    pub fn sync_cloud_snapshot_from(&mut self, other: &Root) {
        self.cloud.connections = other.cloud.connections.clone();
        self.cloud.alert_message = other.cloud.alert_message.clone();
        self.cloud.selected_id = other.cloud.selected_id.clone();
        self.cloud.current_path = other.cloud.current_path.clone();
        self.cloud.entries = other.cloud.entries.clone();
        self.cloud.busy = other.cloud.busy;
        self.cloud.notice = other.cloud.notice.clone();
        self.cloud.filter = other.cloud.filter.clone();
        self.cloud.save_mode = other.cloud.save_mode;
        // 设置面板云管理页（连接列表 + 断连提醒标志）
        self.settings.cloud.connections = other.settings.cloud.connections.clone();
        self.settings.cloud.alert = other.settings.cloud.alert.clone();
    }

    /// 请求重新扫描素材库（云下载素材后由 runner 调用）
    pub fn request_material_scan(&mut self) {
        self.start_material_scan();
    }

    /// 获取编辑器引用
    pub fn editor_ref(&self) -> &editor::Editor {
        &self.editor
    }

    /// 更新播放状态（应在主循环中定期调用）
    ///
    /// 通过无阻塞播放回调（`try_recv_frame`）从播放线程拉取最新帧，
    /// 不再 `lock(playback)`，消除 UI 帧渲染与播放线程的锁争用。
    pub fn update_playback(&mut self) -> Option<f32> {
        if let Some(manager) = &self.playback.manager {
            // 非阻塞拉取最新播放帧：播放线程每帧 try_send，UI 每帧 try_recv。
            // 返回 None 表示无新帧（未播放或线程尚未推送），UI 保持原位置。
            manager.try_recv_frame().map(|frame| frame.tick)
        } else {
            None
        }
    }
}
