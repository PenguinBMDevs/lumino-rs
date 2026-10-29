//! CPU 洋葱皮后端下的不兼容入口守卫（Step 4b）
//!
//! # 为什么要有入口闸
//!
//! Step 4a 只在**切换点**设闸（加载 MIDI 后禁止切换、走带/瀑布流/导出中拒绝切到
//! CPU、纵向卷帘下切 CPU 先自动退回横向）。但用户已经处在 CPU 后端时点进这些入口，
//! 渲染链路只能**静默降级**：
//!
//! | 入口 | CPU 下的实际结果 |
//! |---|---|
//! | 纵向卷帘 | 坐标未转置，CPU 叠加层直接跳过（洋葱皮消失） |
//! | 工程走带 | 走带分支复用 GPU 段表绘制全部音轨（单轨 CPU 模式不参与） |
//! | 全屏瀑布流 | 播放器走 GPU 离屏纹理，CPU 后端不产生帧 |
//! | 右侧栏钢琴瀑布预览 | 同上（离屏 wgpu 键盘纹理） |
//! | 视频导出 | 逐帧渲染依赖 GPU 表面纹理/离屏 compute |
//!
//! 这些不是「能用但效果差」，是「进不到可用状态」。所以入口处就拒绝并说明原因，
//! 而不是让用户面对一个半残界面自己猜。
//!
//! # 单一判定源
//!
//! 判定只有一处：[`Root::is_cpu_onion_backend`]——读主 `Root.settings.hires.backend`
//! （设置应用后与运行时真源 `Host::onion_skin_backend()` 一致）。
//! 文案按**入口**登记在本模块，各 handler 里不再复制「判后端 + 拼文案 + 弹 Toast」
//! 三段式，只保留一次 [`Root::cpu_entry_blocked`] 调用。
//!
//! # 拦截时机（硬约束）
//!
//! 所有拦截都必须发生在 `sidebar.update`、`route`/`AppMode`/面板状态写入**之前**。
//! 否则会出现「路由已经切了一半、按钮已点亮、Toast 才弹出来」的自相矛盾状态——
//! 那比静默降级更难排查。
//!
//! # 只拦「进入」，不拦「退出」
//!
//! `GroupToggled(PianoRoll)` 必须继续可用：它是从走带/瀑布流退回钢琴卷帘的唯一出口，
//! 拦了会把用户锁死在不兼容界面里。

use lumino_core::storage::config::OnionSkinBackend;
use lumino_core::tr;

use crate::right_sidebar::RightSidebarPanel;
use crate::root::Root;
use crate::sidebar::{self, GroupId, RollBarButton, Route};
use crate::toast::ToastLevel;

impl Root {
    /// 主窗口洋葱皮后端是否为 CPU（**入口守卫的唯一判定源**）。
    ///
    /// 读的是主 `Root.settings`：`Host::apply_settings` 里先过
    /// `apply_onion_backend_switch` 再 `root.apply_settings`，被拒绝的请求会被回写成
    /// 运行时真值，因此本判定与 `Host::onion_skin_backend()` 恒等。
    pub(crate) fn is_cpu_onion_backend(&self) -> bool {
        self.settings.hires.backend == OnionSkinBackend::Cpu
    }

    /// 统一拒绝：弹 Warning Toast 并返回 `false`（「本次事件未生效」的统一信号）。
    ///
    /// 返回值恒为 `false` 是刻意的：调用点写成 `return self.reject_cpu_unsupported(..)`
    /// 时语义自解释（不生效），不需要每个 handler 再写一遍 `return false`。
    pub(crate) fn reject_cpu_unsupported(&mut self, message: &'static str) -> bool {
        tracing::warn!("CPU 洋葱皮后端：拒绝不兼容入口（{}）", message);
        self.toast.push(ToastLevel::Warning, message);
        false
    }

    /// 入口闸：CPU 后端时弹 Warning 并返回 `true`（表示**已拦截**，调用方立即返回）。
    ///
    /// 非 CPU 后端**恒返回 `false` 且无任何副作用**——GPU 路径行为与此前完全一致。
    pub(crate) fn cpu_entry_blocked(&mut self, message: &'static str) -> bool {
        if !self.is_cpu_onion_backend() {
            return false;
        }
        self.reject_cpu_unsupported(message);
        true
    }

    /// 侧边栏入口闸：CPU 后端时按事件类型拒绝并返回 `true`。
    ///
    /// 覆盖四类不兼容入口（文案按入口区分，便于用户定位到底哪个功能不可用）：
    /// - 纵向卷帘按钮；
    /// - 工程走带（组切换 / 路由 / 面板三种触发方式都拦，防漏）；
    /// - 全屏瀑布流（组切换）；
    /// - 视频导出（路由 / 面板）。
    ///
    /// 其余事件（含 `GroupToggled(PianoRoll)` 退出路径）原样放行。
    pub(crate) fn cpu_block_sidebar_event(&mut self, event: &sidebar::Event) -> bool {
        if !self.is_cpu_onion_backend() {
            return false;
        }
        let message = match event {
            sidebar::Event::RollBarToggled(RollBarButton::Vertical) => {
                tr!("CPU 洋葱皮后端不支持纵向卷帘")
            }
            sidebar::Event::GroupToggled(GroupId::Project)
            | sidebar::Event::RouteUpdated(Route::Arrangement)
            | sidebar::Event::PanelToggled(Route::Arrangement) => {
                tr!("CPU 洋葱皮后端不支持工程走带视图")
            }
            sidebar::Event::GroupToggled(GroupId::Waterfall) => {
                tr!("CPU 洋葱皮后端不支持全屏瀑布流")
            }
            sidebar::Event::RouteUpdated(Route::VideoExport)
            | sidebar::Event::PanelToggled(Route::VideoExport) => {
                tr!("CPU 洋葱皮后端不支持视频导出")
            }
            _ => return false,
        };
        self.reject_cpu_unsupported(message);
        true
    }

    /// 关闭右侧栏钢琴瀑布流预览面板（切换后端后的界面清退用）。
    ///
    /// 该预览面板渲染的是离屏 wgpu 键盘纹理，CPU 后端下没有帧可画——留着只是空面板。
    /// 只改 `panel_visible`（与既有「再次点击按钮收起面板」路径同款：`active_panel`
    /// 保留，用户切回 GPU 后再点一次即可原位重开），不动 `active_panel` 以免丢失
    /// 面板归属信息。
    ///
    /// 返回是否真的关闭了面板（调用方据此决定要不要追加 Toast）。
    pub(crate) fn close_piano_waterfall_panel(&mut self) -> bool {
        if !self
            .right_sidebar
            .is_panel_active(RightSidebarPanel::PianoWaterfall)
        {
            return false;
        }
        self.right_sidebar.panel_visible = false;
        true
    }
}
