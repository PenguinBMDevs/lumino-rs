//! Root 逐帧动画推进：模式切换（编辑器 ↔ 全屏瀑布流）与平滑滚动
//!
//! 从 `state_update::handle_animation_tick` 抽出的两个子步骤（逐行等价搬迁），原因有二：
//!
//! 1. `state_update.rs` 已到 400 行上限；
//! 2. 更重要的是**耦合**：`AppMode` 只在这里按动画进度翻转，而入口闸只能拦「发起切换」
//!    的那一刻——两者之间隔着若干帧。CPU 洋葱皮后端不支持瀑布流，若守卫只看
//!    `current_mode`，就会漏掉「进瀑布流的动画进行中 → 此刻切到 CPU → 动画收敛后
//!    `current_mode` 变 Waterfall」的竞态（见 [`Root::cancel_cpu_waterfall_transition`]）。
//!    把「翻转 AppMode 的状态机」与守卫放在同一个模块，才不会再次各看各的。

use lumino_core::tr;

use crate::root::Root;
use crate::titlebar::mode_toggle::AppMode;
use crate::toast::ToastLevel;

impl Root {
    /// 推进模式切换动画，并按动画进度同步 `AppMode`。
    ///
    /// `AppMode` 归属阈值（`position >= 0.5` → 瀑布流）与
    /// [`crate::titlebar::mode_toggle`] 的动画语义、以及
    /// `ToggleAnimationState::is_heading_to_waterfall`（`target >= 0.5`）必须同源，
    /// 改一处就要同步另一处。
    pub(crate) fn tick_mode_animation(&mut self) {
        let still_animating = self.state.toggle_animation.update();

        // 纵深防御（CPU 洋葱皮后端）：瀑布流入口已被 `root::handlers::cpu_guard` 闸死，
        // 但「进瀑布流的动画已经在跑 → 此刻切到 CPU」这条时序能绕过它（切换那一刻
        // `current_mode` 还是 Editor）。这里在**模式归属判定之前**取消过渡。
        if self.is_cpu_onion_backend() && self.state.toggle_animation.is_heading_to_waterfall() {
            self.cancel_cpu_waterfall_transition();
            return;
        }

        if still_animating {
            return;
        }
        let target_mode = if self.state.toggle_animation.position >= 0.5 {
            AppMode::Waterfall
        } else {
            AppMode::Editor
        };
        if self.state.current_mode != target_mode {
            self.state.current_mode = target_mode;
        }
    }

    /// CPU 洋葱皮后端下的瀑布流过渡取消（纵深防御）。
    ///
    /// 把过渡归零（`target`/`position`/`velocity`/`active`）并落到编辑器态，
    /// 保证**动画不会再把这个会话带进瀑布流**——即便入口闸被时序绕过。
    ///
    /// 只有「CPU 后端 + 目标朝向瀑布流」才会走到这里：
    /// - 正常路径上，切换闸（`plan_switch` 的 `waterfall` 含过渡态）会先拒绝切到 CPU，
    ///   因此本函数属于兜底；
    /// - GPU 后端永不进入本分支，动画行为零变化。
    fn cancel_cpu_waterfall_transition(&mut self) {
        self.state.toggle_animation.cancel_to_editor();
        self.state.current_mode = AppMode::Editor;
        self.toast
            .push(ToastLevel::Warning, tr!("CPU 洋葱皮后端不支持全屏瀑布流"));
        tracing::warn!("CPU 洋葱皮后端：已取消进行中的瀑布流过渡（后端不支持全屏瀑布流）");
    }

    /// 推进平滑滚动动画并把位置钳制到有效范围。
    ///
    /// 返回是否需要失效编辑器缓存（滚动中视口内容变化）。
    ///
    /// REND-019 实测修复（一次滚轮 → ~2500fps 重绘风暴根因）：
    /// `SmoothScrollAnimation::update` 为 `&self`，**无法清除 `active`**，
    /// 且其收敛判定基于钳制**前**位置——目标被钳制截断（如空工程
    /// `max_scroll=0`、滚轮目标越界）时差值恒 ≥ 阈值：`still_active`
    /// 永真、`active` 永不落幕，`needs_animation`（frame.rs）每帧自驱动重绘。
    /// 此处按「钳制后的实际位置」判定到位/触边，并显式停止动画。
    pub(crate) fn tick_smooth_scroll(&mut self) -> bool {
        let state = &mut self.editor.editor_state;
        let v = &mut state.view;
        let (new_x, new_y, still_active) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);
        // 钳制到有效滚动范围，防止平滑滚动超出键盘/音轨边界
        let max_y = (state.max_scroll.1 - (state.canvas.size_y - v.ruler_height).max(0.0)).max(0.0);
        let max_x =
            (state.max_scroll.0 - (state.canvas.size_x - v.keyboard_width).max(0.0)).max(0.0);
        let clamped_x = new_x.clamp(0.0, max_x);
        let clamped_y = new_y.clamp(0.0, max_y);
        let reached = (clamped_x - v.smooth_scroll.target_x).abs() < v.smooth_scroll.threshold
            && (clamped_y - v.smooth_scroll.target_y).abs() < v.smooth_scroll.threshold;
        let hit_boundary =
            (clamped_x - new_x).abs() > f32::EPSILON || (clamped_y - new_y).abs() > f32::EPSILON;
        v.scroll_x = clamped_x;
        v.scroll_y = clamped_y;
        let animating = still_active && !reached && !hit_boundary;
        if !animating {
            v.smooth_scroll.stop();
        }
        animating
    }
}
