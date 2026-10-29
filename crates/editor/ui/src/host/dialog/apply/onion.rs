//! 洋葱皮渲染后端切换的应用守卫（Step 4a）
//!
//! 「加载 MIDI 后禁止切换后端」是用户硬约束，闸门共三层：
//!
//! 1. `SettingsPanel`（设置页/面板状态）：锁定态忽略 `OnionSkinBackendChanged`，
//!    锁定值由本模块的 [`Host::set_settings_onion_backend_locked`] 注入；
//! 2. **本模块**（主窗口应用点）：锁定或模式不兼容时拒绝，并把被拒值回写成运行时值，
//!    保证「面板显示 == 运行时 == 主 Root.settings」三者一致；
//! 3. `runner::inner::persist`：落盘前再查主窗口文档兜底（对话框先开、MIDI 后载的
//!    时序下，面板字段可能仍是 `backend_locked = false`）。
//!
//! 裁决用纯函数 [`plan_switch`] 表达，副作用由
//! [`Host::apply_onion_backend_switch`] 编排；真正的后端切换仍走
//! `Host::set_onion_skin_backend`（失效 GPU 段表 + CPU 帧缓存），不另开第二条路径。
//!
//! # 与 Step 4b 入口闸的分工
//!
//! 本模块只管**切换点**：不兼容状态下拒绝切到 CPU（含视频导出面板「仅打开未导出」），
//! 切成功后清退已打开的不兼容预览面板（`evict_cpu_incompatible_panels`）。
//! 已经处在 CPU 后端时各模式**入口**的拒绝与 Toast 在
//! `crate::root::handlers::cpu_guard`（判定/文案集中，不在此重复）。

use lumino_core::storage::config::OnionSkinBackend;
use lumino_core::tr;

use crate::host::Host;
use crate::settings::SettingsPanel;
use crate::titlebar::mode_toggle::AppMode;
use crate::toast::ToastLevel;

/// 切换请求的裁决结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchPlan {
    /// 不需要切换（请求值与运行时值相同）
    Keep,
    /// 直接切换
    Switch,
    /// 先强制回横向卷帘，再切换（纵向卷帘 + 目标 CPU）
    ForceHorizontalThenSwitch,
    /// 拒绝：已锁定（加载 MIDI 后）
    RejectLocked,
    /// 拒绝：当前模式/导出与新后端不兼容（走带 / 瀑布流 / 视频导出中）
    RejectIncompatibleMode,
}

/// 裁决上下文（全部来自运行时真源，避免调用方各自拼装判断）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SwitchContext {
    /// 运行时后端（真源：`Host::onion_skin_backend`）
    pub old: OnionSkinBackend,
    /// 设置面板请求的后端
    pub requested: OnionSkinBackend,
    /// 主窗口是否已加载 MIDI（锁定）
    pub locked: bool,
    /// 当前是否纵向卷帘
    pub vertical_roll: bool,
    /// 当前是否为工程走带视图
    pub arrangement: bool,
    /// 当前是否全屏瀑布流
    pub waterfall: bool,
    /// 视频导出是否进行中
    pub exporting: bool,
    /// 视频导出面板是否**仅打开**（未导出）
    ///
    /// 独立于 [`Self::exporting`]：面板打开=用户正在配置（CPU 下无法开始导出），
    /// 关闭用户正在配置的面板比拒绝切换更伤人，因此这种状态也归入「不兼容」。
    pub video_panel_visible: bool,
}

/// 纯裁决：把「请求 + 当前状态」映射成执行计划。
///
/// 判定顺序即优先级：
/// 1. 值相同 → `Keep`（不弹提示，重复保存不产生噪音）；
/// 2. 锁定 → `RejectLocked`（用户硬约束，最高优先）；
/// 3. 目标 CPU 且处于走带/瀑布流/导出中/**视频面板仅打开** → `RejectIncompatibleMode`
///    （**先于**纵向卷帘判定：纵向卷帘的自动横滚是副作用，
///    不能在注定被拒的请求上先改视图）；
/// 4. 目标 CPU 且纵向卷帘 → `ForceHorizontalThenSwitch`；
/// 5. 其余 → `Switch`（切回 GPU 永远安全，无模式限制）。
pub(crate) fn plan_switch(ctx: SwitchContext) -> SwitchPlan {
    if ctx.requested == ctx.old {
        return SwitchPlan::Keep;
    }
    if ctx.locked {
        return SwitchPlan::RejectLocked;
    }
    if ctx.requested == OnionSkinBackend::Cpu {
        if ctx.arrangement || ctx.waterfall || ctx.exporting || ctx.video_panel_visible {
            return SwitchPlan::RejectIncompatibleMode;
        }
        if ctx.vertical_roll {
            return SwitchPlan::ForceHorizontalThenSwitch;
        }
    }
    SwitchPlan::Switch
}

impl Host {
    /// 注入「洋葱皮渲染后端是否锁定」（设置对话框专用）。
    ///
    /// 设置对话框是独立 `Host`，其 Root 里没有 MIDI 文档，无法自行判定锁定态；
    /// 由持有主窗口引用的调用方（`DialogWindow::initialize_ui`）传入
    /// `main_ui.onion_backend_locked()`。锁定后设置页只读展示当前后端，
    /// 且 `OnionSkinBackendChanged` 事件在面板侧被忽略（第二道闸在
    /// [`Host::apply_onion_backend_switch`]，第三道在 runner 的持久化守卫）。
    pub fn set_settings_onion_backend_locked(&mut self, locked: bool) {
        self.root.settings.hires.backend_locked = locked;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用设置面板请求的洋葱皮后端（必须在 `root.apply_settings` **之前**调用）。
    ///
    /// 无论裁决结果如何，都会把 `settings.hires.backend_locked` 写成运行时锁定态，
    /// 并把 `settings.hires.backend` 归一化为运行时真值——随后
    /// `root.apply_settings(settings)` 才能保证主 Root 的设置与渲染行为一致。
    pub(crate) fn apply_onion_backend_switch(&mut self, settings: &mut SettingsPanel) {
        let old = self.onion_skin_backend;
        let locked = self.onion_backend_locked();
        let ctx = SwitchContext {
            old,
            requested: settings.hires.backend,
            locked,
            vertical_roll: self.root.editor.editor_state.is_vertical_roll,
            arrangement: self.root.is_arrangement_mode(),
            waterfall: self.root.state.current_mode == AppMode::Waterfall,
            exporting: self.is_video_exporting(),
            video_panel_visible: self.root.sidebar.video_export_visible,
        };

        match plan_switch(ctx) {
            SwitchPlan::Keep => {}
            SwitchPlan::Switch => {
                self.set_onion_skin_backend(settings.hires.backend);
                if settings.hires.backend == OnionSkinBackend::Cpu {
                    self.evict_cpu_incompatible_panels();
                }
            }
            SwitchPlan::ForceHorizontalThenSwitch => {
                // 纵向卷帘下 CPU 坐标未转置（见 `cpu_onion` 模块头），先退回横向再切。
                self.root.force_horizontal_roll();
                self.set_onion_skin_backend(settings.hires.backend);
                self.root.toast.push(
                    ToastLevel::Info,
                    tr!("已切回横向卷帘：CPU 洋葱皮不支持纵向卷帘"),
                );
                tracing::info!("洋葱皮后端切换: GPU -> CPU，已自动退回横向卷帘");
                self.evict_cpu_incompatible_panels();
            }
            SwitchPlan::RejectLocked => {
                // 拒绝：把被拒值回写成运行时值，避免面板/主 Root 记住一个没生效的值。
                settings.hires.backend = old;
                self.root.toast.push(
                    ToastLevel::Warning,
                    tr!("加载 MIDI 后不可切换洋葱皮渲染方式，请先关闭 MIDI 工程"),
                );
                tracing::warn!("洋葱皮后端切换被拒绝：加载 MIDI 后不可切换（保持 {old:?}）");
            }
            SwitchPlan::RejectIncompatibleMode => {
                settings.hires.backend = old;
                self.root.toast.push(
                    ToastLevel::Warning,
                    tr!("请先返回钢琴卷帘、关闭视频导出面板并停止导出后再切换 CPU 洋葱皮"),
                );
                tracing::warn!(
                    "洋葱皮后端切换被拒绝：当前模式/导出与 CPU 后端不兼容（保持 {old:?}）"
                );
            }
        }

        // 锁定态同步：主 Root.settings 的会话锁定字段由主窗口说了算，
        // 不信任来自对话框的面板值（对话框是独立 Root，无法自行判定文档状态）。
        settings.hires.backend_locked = locked;
    }

    /// 切到 CPU 后的界面清退：右侧栏钢琴瀑布流预览面板不兼容，直接关闭并提示。
    ///
    /// 只清退**预览面板**（它是 GPU 离屏渲染产物，CPU 下没有帧可画，留着只是空面板，
    /// 且用户随时可以切回 GPU 再点一次原位重开）。
    /// **视频导出面板不在此清退**：它承载用户正在配置的导出参数，由
    /// [`plan_switch`] 拒绝切换（`video_panel_visible`）来保住，而不是悄悄关掉。
    fn evict_cpu_incompatible_panels(&mut self) {
        if self.root.close_piano_waterfall_panel() {
            self.root.toast.push(
                ToastLevel::Warning,
                tr!("已关闭钢琴瀑布流预览面板：CPU 洋葱皮不支持该预览"),
            );
            tracing::info!("洋葱皮后端切换: 已关闭不兼容的钢琴瀑布流预览面板");
        }
    }
}

#[cfg(test)]
#[path = "onion/tests.rs"]
mod tests;
