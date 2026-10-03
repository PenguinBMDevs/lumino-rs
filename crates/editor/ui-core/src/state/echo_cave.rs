//! 「关于」页回声洞彩蛋状态机（UI-006）
//!
//! 行为约定（与需求卡逐条对应）：
//! - **进入动效**：进入「关于」页时以**打字机**效果逐字显示当前文案（[`EchoCaveState::begin_typing`]）；
//! - **切换动效**：单击彩蛋文本 ⇒ 当前内容**闪烁** [`ECHO_BLINK_DURATION`]（0.5s）后退出，
//!   下一条文案再以打字机效果进入，索引**循环**；
//! - **连点安全**：闪烁期内的点击一律**忽略**（[`EchoCaveState::on_click`] 返回 `false`）——
//!   这样每次点击**最多**推进一条，且任何时刻最长 [`ECHO_BLINK_DURATION`] 内必然离开闪烁相位，
//!   不会因为连点被反复重置计时而饿死；
//! - **内容内置**：文案为编译期常量 [`ECHO_TEXTS`]，不读配置、不依赖外部文件；不做本地化。
//!
//! 分层纪律：
//! - 状态**不放在** `SettingsPanel`——`Root::apply_settings` 是整面板替换
//!   （`ui/src/root/editor_ops/dialog/settings.rs`），面板内的瞬时动画状态会被搬进主窗口，
//!   使主窗口为一段与它无关的动画持续重绘。故由 `RootState` 持有（与 UI-007 的 `about_egg` 同构）。
//! - 时间推进全部由外部传入 `Instant`，单测可直接构造时间轴，**无需 `sleep`**。
//! - 已揭示前缀在 [`EchoCaveState::update`] 内维护（`view` 只读，保持纯函数，且避免每帧分配）。

use std::time::Instant;

/// 内置彩蛋文案（写死；索引循环切换）
///
/// ⚠️ 全部条目**以非 ASCII 字符起头**：iced 的 `Shaping::Auto` 对**纯 ASCII 串**会退化为
/// `Basic` 整形（`iced_graphics-0.14.0/src/text.rs:325-337`），若打字过程中前缀由 ASCII
/// 切到非 ASCII，同一行字形 advance 可能变化、已绘制前缀出现微抖动。当前文案规避了该情形，
/// 由 `test_all_echo_texts_start_with_non_ascii` 守住。
pub const ECHO_TEXTS: [&str; 5] = [
    "你发现了回声洞。",
    "喂——！……（回音：喂——！）",
    "这里什么都没有，只有回声。",
    "每一个音符，都有回音。",
    "企鹅路过。🐧",
];

/// 打字机速度（字符/秒）
pub const ECHO_CHARS_PER_SECOND: f32 = 20.0;

/// 闪烁退出总时长（秒）——需求卡要求 0.5s
pub const ECHO_BLINK_DURATION: f32 = 0.5;

/// 闪烁明暗半周期（秒）：0.5s / 0.125 = 4 个半周期 = 2 个完整明暗循环
const ECHO_BLINK_PERIOD: f32 = 0.125;

/// 单帧 `dt` 上限（秒）：卡顿/断点后打字不瞬移（与 `about_egg` 同款纪律）
const MAX_DT: f32 = 0.05;

/// 回声洞相位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoPhase {
    /// 空闲：尚未装填，或本次打字已完成（**不要求逐帧驱动**）
    Idle,
    /// 打字机：逐字揭示中
    Typing,
    /// 闪烁退出：当前内容闪烁后切换下一条
    Blinking,
}

/// 「关于」页回声洞彩蛋状态机
#[derive(Debug, Clone)]
pub struct EchoCaveState {
    /// 当前文案索引
    index: usize,
    /// 当前相位
    phase: EchoPhase,
    /// 当前相位已流逝时长（秒）
    phase_elapsed: f32,
    /// 上一帧时刻（dt 计算）
    last_update: Option<Instant>,
    /// 已揭示前缀（`update` 内维护，`view` 只读）
    revealed: String,
}

impl Default for EchoCaveState {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCaveState {
    /// 创建状态机。
    ///
    /// 起始为 [`EchoPhase::Idle`] 且 [`EchoCaveState::is_animating`] 为假——**这一点是刻意的**：
    /// 设置面板每次打开都会新建 `RootState`，若开局即 `Typing`，主窗口的同一状态也会被
    /// 逐帧门控选中，导致 APP 启动后主窗口空转重绘约 1 秒。打字由
    /// [`EchoCaveState::begin_typing`] 在「菜单切到关于页」时显式装填。
    #[must_use]
    pub fn new() -> Self {
        Self {
            index: 0,
            phase: EchoPhase::Idle,
            phase_elapsed: 0.0,
            last_update: None,
            revealed: String::new(),
        }
    }

    /// 装填并开始打字机播放（进入「关于」页时调用）。
    ///
    /// 重复调用等价于**重播**当前文案（离开关于页再回来会重新逐字显示）。
    pub fn begin_typing(&mut self, now: Instant) {
        self.phase = EchoPhase::Typing;
        self.phase_elapsed = 0.0;
        self.last_update = Some(now);
        self.revealed.clear();
    }

    /// 处理一次彩蛋文本点击。
    ///
    /// 返回 `true` 表示本次点击被消费（进入闪烁）；返回 `false` 表示正处于闪烁期，
    /// 点击被忽略（**连点安全**：闪烁相位不会被重置，因而不会被无限推迟）。
    pub fn on_click(&mut self, now: Instant) -> bool {
        match self.phase {
            EchoPhase::Blinking => false,
            EchoPhase::Idle | EchoPhase::Typing => {
                // 极早期点击（前缀为空）时先补全：否则「闪烁空字符串」在观感上
                // 等同于「点击后 0.5s 毫无反应」。
                if self.revealed.is_empty() {
                    self.revealed = ECHO_TEXTS[self.index].to_string();
                }
                self.phase = EchoPhase::Blinking;
                self.phase_elapsed = 0.0;
                self.last_update = Some(now);
                true
            }
        }
    }

    /// 推进动画；返回是否仍在动画中。
    ///
    /// `dt` 上限钳 [`MAX_DT`]：卡顿/断点后不会瞬移。
    pub fn update(&mut self, now: Instant) -> bool {
        if !self.is_animating() {
            self.last_update = Some(now);
            return false;
        }

        let dt = match self.last_update {
            Some(prev) => now
                .saturating_duration_since(prev)
                .as_secs_f32()
                .min(MAX_DT),
            None => 0.0,
        };
        self.last_update = Some(now);
        self.phase_elapsed += dt;

        match self.phase {
            EchoPhase::Typing => {
                let total = ECHO_TEXTS[self.index].chars().count();
                // 必须按 **char** 边界截取：文案含 CJK（3 字节）与 emoji（4 字节），
                // 字节切片 `&s[..n]` 会 panic 在非 char 边界。
                let shown = (self.phase_elapsed * ECHO_CHARS_PER_SECOND) as usize;
                self.revealed = ECHO_TEXTS[self.index].chars().take(shown).collect();
                if shown >= total {
                    self.revealed = ECHO_TEXTS[self.index].to_string();
                    self.phase = EchoPhase::Idle;
                    self.phase_elapsed = 0.0;
                    return false;
                }
            }
            EchoPhase::Blinking => {
                if self.phase_elapsed >= ECHO_BLINK_DURATION {
                    // 循环切换：下一条以打字机效果进入
                    self.index = (self.index + 1) % ECHO_TEXTS.len();
                    self.phase = EchoPhase::Typing;
                    self.phase_elapsed = 0.0;
                    self.revealed.clear();
                }
            }
            EchoPhase::Idle => return false,
        }

        true
    }

    /// 当前应显示的文本（已揭示前缀）
    #[must_use]
    pub fn text(&self) -> &str {
        &self.revealed
    }

    /// 当前不透明度（闪烁方波；`Typing`/`Idle` 恒为 1.0）
    ///
    /// 用 alpha 而非「隐藏控件」表达闪烁：隐藏会把行高/宽度交给布局重算，
    /// 闪烁期间整页会上下呼吸；alpha=0 保留布局盒，**零重排**。
    #[must_use]
    pub fn opacity(&self) -> f32 {
        if self.phase != EchoPhase::Blinking {
            return 1.0;
        }
        let half_periods = (self.phase_elapsed / ECHO_BLINK_PERIOD).floor() as u32;
        if half_periods.is_multiple_of(2) {
            1.0
        } else {
            0.0
        }
    }

    /// 是否处于需要逐帧驱动的状态（打字 / 闪烁）
    #[must_use]
    pub fn is_animating(&self) -> bool {
        matches!(self.phase, EchoPhase::Typing | EchoPhase::Blinking)
    }

    /// 当前相位（测试与日志用）
    #[must_use]
    pub fn phase(&self) -> EchoPhase {
        self.phase
    }

    /// 当前文案索引（测试与日志用）
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// 当前文案的完整字符数（测试用）
    #[must_use]
    pub fn current_total_chars(&self) -> usize {
        ECHO_TEXTS[self.index].chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// 以 16ms 帧推进，直到不再动画或超过 `max_frames`；返回消耗帧数
    fn drive_until_settled(
        state: &mut EchoCaveState,
        now: &mut Instant,
        max_frames: usize,
    ) -> usize {
        for frame in 0..max_frames {
            *now += ms(16);
            if !state.update(*now) {
                return frame + 1;
            }
        }
        max_frames
    }

    /// 按 ≤16ms 分帧推进指定时长。
    ///
    /// **必须分帧**：`update` 内部的 `dt` 上限钳制（`MAX_DT`）会丢弃单次超大间隔，
    /// 一次 `now += 520ms; update(now)` 只推进 50ms —— 这正是「卡顿后不瞬移」的设计意图。
    fn advance_for(state: &mut EchoCaveState, now: &mut Instant, duration: Duration) -> bool {
        let mut remaining = duration;
        let mut still = state.is_animating();
        while remaining > Duration::ZERO {
            let step = remaining.min(ms(16));
            *now += step;
            remaining -= step;
            still = state.update(*now);
        }
        still
    }

    #[test]
    fn test_new_state_is_idle_and_not_animating() {
        // 关键护栏：新建状态**不得**要求逐帧驱动，否则主窗口会在启动后空转重绘
        let state = EchoCaveState::new();
        assert_eq!(state.phase(), EchoPhase::Idle);
        assert!(!state.is_animating());
        assert_eq!(state.text(), "");
        assert_eq!(state.opacity(), 1.0);
    }

    #[test]
    fn test_begin_typing_reveals_monotonically() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        assert_eq!(state.phase(), EchoPhase::Typing);
        assert!(state.is_animating());
        assert_eq!(state.text(), "", "起振点为空，不得预先显示内容");

        let mut prev = 0;
        // 4 × 50ms = 200ms @20cps ≈ 4 字（第 0 条共 8 字，故此刻应「露了一半」）
        for _ in 0..4 {
            now += ms(50);
            state.update(now);
            let shown = state.text().chars().count();
            assert!(shown >= prev, "已揭示字符数应单调不减: {prev} -> {shown}");
            prev = shown;
        }
        assert!(prev > 0, "推进 200ms 后应已揭示若干字符");
        assert!(
            prev < state.current_total_chars(),
            "200ms @20cps 不足以揭示完整文案（否则打字机不可见），实际 {prev} 字"
        );
    }

    #[test]
    fn test_typing_completes_and_stops_animating() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);

        let frames = drive_until_settled(&mut state, &mut now, 600);
        assert_eq!(state.phase(), EchoPhase::Idle, "打字应自然收尾");
        assert!(!state.is_animating(), "收尾后不得再要求逐帧驱动");
        assert_eq!(
            state.text(),
            ECHO_TEXTS[0],
            "收尾时必须显示完整文案（不得少字）"
        );
        assert!(frames < 600, "应在有界帧数内收尾，实际 {frames} 帧");
    }

    #[test]
    fn test_reveal_is_char_boundary_safe_with_emoji() {
        // 末条含 emoji（4 字节）与中文（3 字节）：逐帧推进全程不得 panic，
        // 且每个中间态都必须是完整文案的合法前缀
        let mut state = EchoCaveState::new();
        state.index = ECHO_TEXTS.len() - 1;
        let mut now = Instant::now();
        state.begin_typing(now);

        let target = ECHO_TEXTS[ECHO_TEXTS.len() - 1];
        let mut seen = 0;
        for _ in 0..600 {
            now += ms(16);
            let still = state.update(now);
            let shown = state.text().chars().count();
            assert!(
                target.starts_with(state.text()),
                "中间态必须是完整文案的前缀: {:?}",
                state.text()
            );
            seen = shown;
            if !still {
                break;
            }
        }
        assert_eq!(state.text(), target);
        assert!(seen > 0);
    }

    #[test]
    fn test_click_freezes_reveal_and_enters_blink() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        now += ms(100);
        state.update(now);
        let frozen = state.text().to_string();

        assert!(state.on_click(now), "打字期点击应被消费");
        assert_eq!(state.phase(), EchoPhase::Blinking);
        assert!(state.is_animating());
        assert_eq!(state.text(), frozen, "闪烁期内容必须冻结（不继续揭示）");
        assert_eq!(state.index(), 0, "闪烁期不得提前切换索引");
    }

    #[test]
    fn test_click_early_fills_text_before_blink() {
        let mut state = EchoCaveState::new();
        let now = Instant::now();
        state.begin_typing(now);
        // 尚未推进任何帧 ⇒ 前缀为空
        assert_eq!(state.text(), "");
        assert!(state.on_click(now));
        assert_eq!(
            state.text(),
            ECHO_TEXTS[0],
            "极早期点击应先补全，避免「闪烁空字符串」被观感为点击无反应"
        );
    }

    #[test]
    fn test_blink_opacity_toggles_square_wave() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        now += ms(200);
        state.update(now);
        assert!(state.on_click(now));

        assert_eq!(
            state.opacity(),
            1.0,
            "闪烁起始应为可见（避免切入瞬间闪一下）"
        );
        // 采样点 = 各半周期中点（0.0625 / 0.1875 / 0.3125 / 0.4375s）；
        // 用 advance_for 分帧推进——单次大步会被 dt 上限拦下，读到的相位时间是错的
        let mut samples = Vec::new();
        for delta in [62_u64, 125, 125, 125] {
            advance_for(&mut state, &mut now, ms(delta));
            samples.push(state.opacity());
        }
        assert_eq!(
            samples,
            vec![1.0, 0.0, 1.0, 0.0],
            "闪烁应为明暗交替方波（切入即显示，随后交替），实际 {samples:?}"
        );
    }

    #[test]
    fn test_blink_advances_index_once_and_cycles() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        drive_until_settled(&mut state, &mut now, 600);

        for expected in 0..ECHO_TEXTS.len() {
            assert_eq!(state.index(), expected);
            assert!(state.on_click(now));
            // 推进恰好超过闪烁时长（分帧，否则被 dt 上限拦下）
            advance_for(&mut state, &mut now, ms(520));
            assert_eq!(state.phase(), EchoPhase::Typing, "闪烁后应进入打字机");
            assert_eq!(
                state.index(),
                (expected + 1) % ECHO_TEXTS.len(),
                "索引应 +1 并循环"
            );
        }
        assert_eq!(state.index(), 0, "走完全部文案应回到第 0 条");
    }

    #[test]
    fn test_rapid_clicks_during_blink_do_not_stall() {
        // 验收 4「快速连续点击不卡死」的直接护栏：
        // 闪烁期内狂点 50 次，仍必须在 ≤ ECHO_BLINK_DURATION 内离开闪烁，且索引恰好 +1
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        drive_until_settled(&mut state, &mut now, 600);
        assert_eq!(state.phase(), EchoPhase::Idle);

        assert!(state.on_click(now), "首次点击进入闪烁");
        let index_before = state.index();

        for _ in 0..50 {
            now += ms(2);
            assert!(
                !state.on_click(now),
                "闪烁期点击必须被忽略（否则计时被反复重置 ⇒ 观感卡死）"
            );
            state.update(now);
            if state.phase() != EchoPhase::Blinking {
                break;
            }
        }

        // 剩余时间推进完闪烁
        let frames = drive_until_settled(&mut state, &mut now, 200);
        assert_ne!(
            state.phase(),
            EchoPhase::Blinking,
            "连点不得把状态钉死在闪烁相位"
        );
        assert_eq!(
            state.index(),
            (index_before + 1) % ECHO_TEXTS.len(),
            "50 次连点只允许推进 **1** 条文案"
        );
        assert!(frames < 200);
    }

    #[test]
    fn test_click_while_blinking_does_not_double_advance() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        drive_until_settled(&mut state, &mut now, 600);

        assert!(state.on_click(now));
        advance_for(&mut state, &mut now, ms(100));
        assert!(!state.on_click(now), "闪烁期第二次点击应被忽略");
        advance_for(&mut state, &mut now, ms(450));
        assert_eq!(state.index(), 1, "只应推进一条");
    }

    #[test]
    fn test_every_phase_reaches_non_animating_within_bound() {
        // 性能护栏：任何相位都必须有界收敛，否则会变成对话框常驻逐帧重绘
        // （仓库历史事故：RedrawRequested 自循环致 GPU 满载）
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);

        // 走满一轮：打字 → 点击 → 闪烁 → 打字 → 直到空闲
        let mut total_frames = 0;
        for round in 0..ECHO_TEXTS.len() {
            total_frames += drive_until_settled(&mut state, &mut now, 400);
            assert!(
                !state.is_animating(),
                "第 {round} 轮打字必须在 400 帧内收敛"
            );
            assert!(state.on_click(now));
            advance_for(&mut state, &mut now, ms(520));
        }
        total_frames += drive_until_settled(&mut state, &mut now, 400);
        assert!(!state.is_animating(), "整轮循环后必须回到不动的状态");
        assert!(total_frames < 3_000, "总帧数应有界，实际 {total_frames}");
    }

    #[test]
    fn test_dt_is_clamped_after_a_stall() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        // 模拟窗口被挂起 10 秒：dt 必须钳到 MAX_DT，打字不得瞬移到底
        now += Duration::from_secs(10);
        state.update(now);
        let shown = state.text().chars().count();
        assert!(
            shown <= (MAX_DT * ECHO_CHARS_PER_SECOND).ceil() as usize,
            "单帧揭示不得超过 dt 上限对应的字符数，实际 {shown}"
        );
    }

    #[test]
    fn test_begin_typing_restarts_reveal() {
        let mut state = EchoCaveState::new();
        let mut now = Instant::now();
        state.begin_typing(now);
        drive_until_settled(&mut state, &mut now, 600);
        assert_eq!(state.text(), ECHO_TEXTS[0]);

        // 重新进入关于页 ⇒ 重播
        now += ms(500);
        state.begin_typing(now);
        assert_eq!(state.phase(), EchoPhase::Typing);
        assert_eq!(state.text(), "", "重播应从头逐字揭示");
        assert_eq!(state.index(), 0, "重播不得改变当前文案索引");
    }

    #[test]
    fn test_texts_are_non_empty() {
        for (i, text) in ECHO_TEXTS.iter().enumerate() {
            assert!(!text.is_empty(), "第 {i} 条内置文案不得为空");
            assert!(
                text.chars().count() >= 4,
                "第 {i} 条文案过短，打字机效果不可见"
            );
        }
    }

    #[test]
    fn test_all_echo_texts_start_with_non_ascii() {
        // iced 的 Shaping::Auto 对纯 ASCII 串走 Basic 整形（无逐字形回退）；
        // 若打字前缀由 ASCII 切到非 ASCII，同行 advance 可能变化 ⇒ 前缀微抖动。
        // 保持全部文案以非 ASCII 起头即可规避。
        for (i, text) in ECHO_TEXTS.iter().enumerate() {
            let first = text.chars().next().expect("文案非空已由上一用例保证");
            assert!(
                !first.is_ascii(),
                "第 {i} 条文案以 ASCII 起头（{first:?}），会在打字中途触发整形切换"
            );
        }
    }
}
