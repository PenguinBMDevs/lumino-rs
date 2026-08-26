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
            // 读取 `last_frame` 缓存（而非消费有界通道 `try_recv_frame`）：
            // 播放中每秒推送上百帧，有界通道（容量 8）极易饱和，状态切换
            // （Pause/Stop/自动停止）帧在 `try_send` 满时会被丢弃，导致 UI 永远
            // 收不到「已暂停/已停止」信号、按钮卡在「播放中」。`last_frame` 缓存
            // 每次推送都会写入，永不丢帧，是播放状态唯一可靠的真相源。
            if let Some(frame) = manager.last_frame() {
                // 仅在引擎真正停止（播到轨尾自动停止 / 显式 Stop）时复位 `is_playing`。
                //
                // 关键：不在此处把 `is_playing` 置 true，也不因 Playing 帧把它翻回 true。
                // 播放/暂停/停止的「意图状态」由 `do_play/do_pause/do_stop` 同步设置，
                // 这里只做「自动停止」这一件 UI 无法预知的复位。
                // 否则在「Pause 命令已发出、但播放线程尚未处理、帧仍为 Playing」的窗口内，
                // 旧 Playing 帧会把刚置 false 的 `is_playing` 翻回 true，造成
                // 「按空格声音已停、按钮却仍显示播放、需再按一次才更新」的竞态。
                if frame.state == crate::playback::PlaybackState::Stopped {
                    self.toolbar.is_playing = false;
                }
                Some(frame.tick)
            } else {
                None
            }
        } else {
            None
        }
    }
}
