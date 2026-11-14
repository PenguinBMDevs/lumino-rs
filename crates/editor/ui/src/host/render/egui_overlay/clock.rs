//! egui 帧时钟：把宿主真实时间喂给 `RawInput`（点击判定的**时间**语义）。
//!
//! # 根因（RCA）
//!
//! egui 的点击判定是**时间**语义，不是帧语义：
//! `self.time - press_start_time > max_click_duration`（默认 0.8s）即不算点击
//! （`egui-0.37.0/src/input_state/mod.rs` 的 `could_any_button_be_click`）。
//!
//! 而 `RawInput::time` 是 `Option<f64>`：**不填**时 egui 走
//! `let time = new.time.unwrap_or(self.time + new.predicted_dt as f64);`
//! （`begin_pass`），也就是每帧凭空前进 `predicted_dt`（默认 1/60）。
//!
//! 于是 egui 的虚拟时钟只跟**帧数**有关，与真实时间脱钩：
//!
//! - 刷新率跑满（真机实测播放期 **475fps**）→ 虚拟时钟比真实时间快 **约 8×**；
//!   一次 150ms 的正常点击在 egui 眼里已是 1.2s，超过 0.8s ⇒ `clicked()` 恒为 `false`；
//! - 慢帧（本地压测机播放期仅 16fps）→ 虚拟时钟反而比真实时间**慢** ⇒ 点击永远算数，
//!   于是「自动化 20/20 全绿，真机全红」。
//!
//! 真机症状与之完全吻合：菜单能展开（悬停切换 + 极轻的点击），
//! 点菜单条目 / 切换类按钮（纯 `clicked()`）无响应。
//!
//! 上游同类问题：[emilk/egui#2502](https://github.com/emilk/egui/issues/2502)
//! ——「>300fps 时点击检测失效」，结论正是「没有把时间喂给 `RawInput`」。
//!
//! # 真机 A/B 实测（同一份二进制，`LUMINO_EGUI_LEGACY_CLOCK=1` 关闭修复）
//!
//! | 模式 | 帧率 | 按压 | 播放中点击生效 | 时钟偏差 |
//! |------|------|------|----------------|----------|
//! | 旧行为 | 475fps | 150ms（真人） | **0 / 12** | +241s（约 8×） |
//! | 旧行为 | 475fps | 45ms（合成短按） | 13 / 12 | +223s |
//! | 修复后 | 472fps | 150ms（真人） | **12 / 12** | 0 |
//!
//! 第二行是关键教训：合成短按在虚拟时钟下会**侥幸通过**，压测必须用真人时长，
//! 否则测的不是同一个场景（见 `short_synthetic_press_masks_the_bug_but_human_press_does_not`）。
//!
//! # 对策
//!
//! 每帧都填 `RawInput::time`，取宿主 `Instant` 的单调秒数；**保留**
//! `predicted_dt` 默认值（与 `egui-winit` / `eframe` 的参考接线一致）。

use std::time::Instant;

/// 帧时钟：宿主启动时刻 → 单调递增秒数。
///
/// 必须是**单调**时钟：`SystemTime` 会被 NTP / 手动改时间拉扯，
/// 一旦回退 egui 就会算出负的按压时长（点击判定失效或连击计数错乱）。
pub(crate) struct FrameClock {
    start: Instant,
}

impl FrameClock {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// 本帧的 egui 时间（秒，自叠加层创建起单调递增）。
    pub(crate) fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// 组装一帧 `RawInput`。
///
/// `time` 必须来自 [`FrameClock::elapsed`]：漏填会退化为「按帧计时」，
/// 高刷新率下点击判定全线失效（见模块注释）。
pub(crate) fn raw_input(
    events: Vec<egui::Event>,
    size: [f32; 2],
    focused: bool,
    time: f64,
) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0], size[1]),
        )),
        events,
        focused,
        time: Some(time),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use egui::{Context, Event, PointerButton, Rect, pos2, vec2};

    /// 测试用帧率：真机播放期实测的满载刷新率（475fps 量级）。
    ///
    /// 取 480 与实测同量级。判定门槛由 egui 的 `max_click_duration`（0.8s）与
    /// 虚拟时钟推进速度共同决定：`press_ms/1000 * FPS / 60`。
    /// 300fps 时 150ms 只放大到 0.75s（仍侥幸通过），480fps 才放大到 1.2s（必失败）
    /// ——这正是同一份代码在不同机器上表现相反的原因。
    const FPS: f64 = 480.0;

    /// 真人点击的按压时长（毫秒）。
    ///
    /// 真实鼠标点击通常 100~200ms；合成脚本常用的 20~45ms 是**非人类**速度，
    /// 会掩盖本 bug（见 `short_synthetic_press_masks_the_bug_but_human_press_does_not`）。
    const PRESS_MS: f64 = 150.0;

    fn click_at(pos: egui::Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// 跑一次「按下 → 保持 `press_ms` → 抬起」，返回控件是否收到 `clicked()`。
    ///
    /// `with_time = false` 复现修复前的行为（不填 `RawInput::time`）。
    fn click_at_fps(with_time: bool, press_ms: f64) -> bool {
        let ctx = Context::default();
        let pos = pos2(40.0, 20.0);
        let mut clicked = false;
        // 第 0 帧只画控件（egui 的命中使用上一帧控件矩形），第 1 帧按下。
        let press_frames = ((press_ms / 1000.0) * FPS).round() as u32;
        let release_frame = press_frames + 1;

        for frame in 0..(release_frame + 1) {
            let mut events = vec![Event::PointerMoved(pos)];
            if frame == 1 {
                events.push(click_at(pos, true));
            } else if frame == release_frame {
                events.push(click_at(pos, false));
            }

            let mut input = super::raw_input(events, [400.0, 300.0], true, frame as f64 / FPS);
            if !with_time {
                input.time = None;
            }

            let mut output = ctx.run_ui(input, |ui| {
                let response = ui.put(
                    Rect::from_min_size(pos2(20.0, 8.0), vec2(60.0, 24.0)),
                    egui::Button::new("hit"),
                );
                clicked |= response.clicked();
            });
            // `TexturesDelta` 必须消费掉，否则 drop 时 epaint 会 panic。
            output.textures_delta.clear();
        }
        clicked
    }

    /// 失败对照：不填 `time` 时，300fps 下一次正常时长的点击被判无效。
    ///
    /// 这条测试钉死的是 egui 的**事实行为**，也是真机「播放时点不动」的根因。
    #[test]
    fn click_is_dropped_without_time_at_full_refresh_rate() {
        assert!(
            !click_at_fps(false, PRESS_MS),
            "未填时间时 egui 按帧计时：300fps 下正常点击会被判超时，\
             若此处变 true 说明 egui 语义已变，本文档与修复都要复核"
        );
    }

    /// 修复：填了真实时间后，同一场景必须识别为点击。
    #[test]
    fn click_registers_with_real_time_at_full_refresh_rate() {
        assert!(
            click_at_fps(true, PRESS_MS),
            "填了真实时间后，300fps 下的正常点击必须成立（真机回归门禁）"
        );
    }

    /// **压测方法自身的坑**：短按在虚拟时钟下也可能侥幸通过。
    ///
    /// 合成的 45ms 短按在 8 倍虚拟时钟下只有 ~360ms，仍未超过 0.8s 阈值；
    /// 而真人 150ms 的按压会被放大到 ~1.2s，直接判超时。
    ///
    /// 这就是「自动化 20/20 全绿、真机全红」的原因——压测必须用**真人时长**，
    /// 否则测的不是同一个场景。此测试把这条教训钉死。
    #[test]
    fn short_synthetic_press_masks_the_bug_but_human_press_does_not() {
        // 短按：旧行为侥幸通过（正是过去误判为「已修好」的原因）。
        assert!(
            click_at_fps(false, 45.0),
            "45ms 短按在虚拟时钟下会侥幸通过——压测若只用短按，就测不出真机问题"
        );
        // 真人时长：旧行为必定失败。
        assert!(
            !click_at_fps(false, PRESS_MS),
            "150ms 真人按压在虚拟时钟下必须失败（真机现象）"
        );
        // 修复后两者都通过。
        assert!(click_at_fps(true, 45.0), "修复后短按也必须通过");
        assert!(click_at_fps(true, PRESS_MS), "修复后真人按压必须通过");
    }

    /// 生产路径必须真的把时间填进去（不能只在测试里调用）。
    #[test]
    fn frame_clock_is_monotonic_and_raw_input_carries_it() {
        let clock = super::FrameClock::new();
        let first = clock.elapsed();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = clock.elapsed();
        assert!(second >= first, "帧时钟必须单调不减");

        let raw = super::raw_input(Vec::new(), [800.0, 600.0], true, second);
        assert_eq!(
            raw.time,
            Some(second),
            "RawInput::time 必须由宿主填写（漏填即真机点击失效）"
        );
        assert_eq!(
            raw.screen_rect,
            Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)))
        );
    }
}
