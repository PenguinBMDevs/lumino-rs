//! AboutEgg 彩蛋单测（从 about_egg.rs 拆出）

use super::*;

/// 进程级标志使彩蛋用例之间存在共享状态：统一串行化，避免并行测试互相污染。
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 测试用构造：忽略进程级消失标志，保证用例可重复执行
fn idle_state() -> AboutEggState {
    let mut state = AboutEggState::new();
    state.phase = EggPhase::Idle;
    state
}

/// 构造一个视口已注入、点击点已知的状态机
fn ready_state() -> AboutEggState {
    let mut state = idle_state();
    state.set_viewport(720.0, 540.0);
    state.set_click_point(Some((305.0, 121.0)));
    state
}

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn test_impact_signal_is_one_shot_at_landing() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();
    for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
        state.on_logo_click(now);
        now += ms(50);
    }

    // 起飞后、落地前不得有信号
    assert!(
        !state.take_impact_signal(),
        "序列开始前/落地前不应有落地信号"
    );

    let mut landed_at_impact = false;
    for _ in 0..600 {
        now += ms(16);
        let still = state.update(now);
        let signalled = state.take_impact_signal();
        if signalled {
            assert_eq!(
                state.phase(),
                EggPhase::Impact,
                "落地信号必须与进入 Impact 相位同帧"
            );
            landed_at_impact = true;
            // 消费式：同一帧再取应为 false
            assert!(!state.take_impact_signal(), "落地信号必须只能取走一次");
        }
        if !still {
            break;
        }
    }
    assert!(landed_at_impact, "整段序列应产生恰好一次落地信号");
    assert!(!state.take_impact_signal(), "序列收尾后不应残留落地信号");
}

#[test]
fn test_single_click_shakes_then_settles() {
    let _guard = lock();
    let mut state = ready_state();
    let t0 = Instant::now();

    assert!(state.on_logo_click(t0), "首次点击应被消费（触发晃动）");
    assert_eq!(state.phase(), EggPhase::Shake);
    assert_eq!(state.clicks(), 1);

    // 起振点为 0（不会在相位切换处跳变）
    assert!(state.shake_offset().abs() < f32::EPSILON);

    // 晃动全程位移不超振幅，且相位最终回到静止
    let mut now = t0;
    let mut peak = 0.0_f32;
    for _ in 0..40 {
        now += ms(16);
        state.update(now);
        peak = peak.max(state.shake_offset().abs());
        if state.phase() == EggPhase::Idle {
            break;
        }
    }
    assert_eq!(state.phase(), EggPhase::Idle, "晃动应在一个周期内收住");
    assert!(peak > 0.0, "晃动期间应产生可见位移");
    assert!(
        peak <= ABOUT_LOGO_SHAKE_AMPLITUDE,
        "晃动位移不得超振幅: {peak}"
    );
    assert_eq!(state.shake_offset(), 0.0, "静止相位位移恒为 0");
}

#[test]
fn test_click_timeout_resets_counter() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();

    // 14 次连点（间隔 100ms，不超时）
    for _ in 0..14 {
        state.on_logo_click(now);
        now += ms(100);
    }
    assert_eq!(state.clicks(), 14);
    assert_eq!(state.phase(), EggPhase::Shake);

    // 停顿 3s（超过 2s 阈值）后再点：计数应从 1 重新累计
    now += Duration::from_secs(3);
    assert!(state.on_logo_click(now));
    assert_eq!(state.clicks(), 1, "超过超时阈值后计数应清零重新累计");
    assert!(!state.is_airborne(), "不应因未满阈值而触发彩蛋");
}

#[test]
fn test_fifteen_clicks_trigger_full_sequence() {
    let _guard = lock();
    reset_logo_vanished();
    let mut state = ready_state();
    let mut now = Instant::now();

    for i in 0..ABOUT_EGG_CLICK_THRESHOLD {
        assert!(state.on_logo_click(now), "第 {} 次点击应被消费", i + 1);
        now += ms(100);
    }
    assert_eq!(state.phase(), EggPhase::Spin, "满阈值应进入快速转动");

    // 逐帧推进整段序列，记录相位出现顺序（含收尾的 Vanished）
    let mut order: Vec<EggPhase> = Vec::new();
    for _ in 0..600 {
        now += ms(16);
        let still_animating = state.update(now);
        if order.last() != Some(&state.phase()) {
            order.push(state.phase());
        }
        if !still_animating {
            break;
        }
    }

    assert_eq!(state.phase(), EggPhase::Vanished, "序列应完整播放至消失");
    assert_eq!(
        order,
        vec![
            EggPhase::Spin,
            EggPhase::Detach,
            EggPhase::Fall,
            EggPhase::Impact,
            EggPhase::FadeOut,
            EggPhase::Vanished,
        ],
        "相位顺序应为 转动→脱离→下落→打入位→渐隐→消失"
    );
    assert!(logo_has_vanished(), "序列结束应写入进程级消失标志");
    assert!(!state.is_visible_in_page());
    assert!(!state.is_airborne());
    assert!(!state.is_animating(), "消失后不得再要求逐帧驱动");
}

#[test]
fn test_fall_target_is_above_window_center() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();
    for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
        state.on_logo_click(now);
        now += ms(50);
    }

    // 推进到下落开始的那一刻，检查脱离结束位置
    let mut checked = false;
    for _ in 0..200 {
        now += ms(16);
        if !state.update(now) {
            break;
        }
        if state.phase() == EggPhase::Fall {
            let (_, y) = state.position();
            assert!(
                y + ABOUT_LOGO_BOX <= 540.0 * 0.5 + 0.5,
                "脱离结束位置必须落在窗口中轴 Y 以上: y={y}"
            );
            checked = true;
            break;
        }
    }
    assert!(checked, "应在有限帧内进入下落相位");
}

#[test]
fn test_fall_accelerates_monotonically() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();
    for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
        state.on_logo_click(now);
        now += ms(50);
    }

    let mut prev_y: Option<f32> = None;
    let mut prev_delta = 0.0_f32;
    let mut samples = 0;
    for _ in 0..200 {
        now += ms(16);
        if !state.update(now) {
            break;
        }
        if state.phase() != EggPhase::Fall {
            continue;
        }
        let (_, y) = state.position();
        if let Some(py) = prev_y {
            let delta = y - py;
            assert!(delta >= 0.0, "下落不应反向");
            if samples > 0 {
                assert!(
                    delta >= prev_delta - 1e-3,
                    "自然加速：每帧位移应单调不减（第 {samples} 帧）"
                );
            }
            prev_delta = delta;
            samples += 1;
        }
        prev_y = Some(y);
    }
    assert!(samples >= 3, "下落过程应至少采到 3 帧");
}

#[test]
fn test_click_ignored_while_sequence_running() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();
    for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
        state.on_logo_click(now);
        now += ms(50);
    }
    assert_eq!(state.phase(), EggPhase::Spin);

    assert!(!state.on_logo_click(now), "序列进行中的点击应被忽略");
    assert_eq!(state.clicks(), 0, "序列进行中不得重新累计计数");
    assert_eq!(state.phase(), EggPhase::Spin, "序列相位不应被打断");
}

#[test]
fn test_repeated_shake_does_not_trip_hard_timeout() {
    let _guard = lock();
    let mut state = ready_state();
    let start = Instant::now();
    let mut now = start;

    // 反复晃动（累计远超硬超时阈值），但始终未满 15 次
    for _ in 0..13 {
        state.on_logo_click(now);
        now += ms(500);
    }
    assert!(
        now.duration_since(start) > ABOUT_EGG_HARD_TIMEOUT,
        "构造条件：晃动累计时长已超过硬超时阈值"
    );

    // 再补 2 次触发序列：不应因为"晃动累计时长"被误判超时而立即收尾
    state.on_logo_click(now);
    now += ms(100);
    state.on_logo_click(now);
    assert_eq!(state.phase(), EggPhase::Spin);

    let mut reached_fall = false;
    for _ in 0..120 {
        now += ms(16);
        if !state.update(now) {
            break;
        }
        if state.phase() == EggPhase::Fall {
            reached_fall = true;
            break;
        }
    }
    assert!(reached_fall, "晃动累计时长不应触发序列硬超时兜底");
}

#[test]
fn test_vanished_logo_absent_on_reopen_but_state_is_marked() {
    let _guard = lock();
    reset_logo_vanished();
    assert!(!logo_has_vanished());

    mark_logo_vanished();
    // 模拟「关掉设置面板 → 重新打开」：全新状态机应直接处于已消失
    let mut reopened = AboutEggState::new();
    assert_eq!(reopened.phase(), EggPhase::Vanished);
    assert!(!reopened.is_visible_in_page());
    assert!(
        !reopened.on_logo_click(Instant::now()),
        "消失后点击应被忽略"
    );

    reset_logo_vanished();
}

#[test]
fn test_spin_fit_scale_compensates_rotated_bounding_box() {
    let _guard = lock();
    let mut state = idle_state();
    state.spin_angle = 0.0;
    assert!((state.spin_fit_scale() - 1.0).abs() < 1e-3);

    state.spin_angle = std::f32::consts::FRAC_PI_4;
    let at_45 = state.spin_fit_scale();
    assert!(
        (1.25..=1.35).contains(&at_45),
        "45° 时的尺寸补偿应约为 1.30，实际 {at_45}"
    );

    // 补偿后绘制尺寸恒定：Svg 固定边长 / AABB 长边 恒等于静止时的比例
    for step in 0..24 {
        state.spin_angle = std::f32::consts::TAU * step as f32 / 24.0;
        let (sin, cos) = (state.spin_angle.sin().abs(), state.spin_angle.cos().abs());
        let aabb = (LOGO_ASPECT_W * cos + LOGO_ASPECT_H * sin)
            .max(LOGO_ASPECT_W * sin + LOGO_ASPECT_H * cos);
        let fitted = ABOUT_LOGO_SIZE * state.spin_fit_scale();
        let drawn_height = LOGO_ASPECT_H * fitted / aabb;
        assert!(
            (drawn_height - ABOUT_LOGO_SIZE).abs() < 0.5,
            "旋转过程中绘制高度应恒定，实际 {drawn_height}"
        );
    }
}

#[test]
fn test_position_stays_inside_viewport() {
    let _guard = lock();
    let mut state = ready_state();
    let mut now = Instant::now();
    for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
        state.on_logo_click(now);
        now += ms(50);
    }

    for _ in 0..600 {
        now += ms(16);
        if !state.update(now) {
            break;
        }
        let (x, y) = state.position();
        assert!(
            (-0.5..=720.0 - ABOUT_LOGO_BOX + 0.5).contains(&x),
            "X 越界: {x}"
        );
        assert!(
            (-0.5..=540.0 - ABOUT_LOGO_BOX + 0.5).contains(&y),
            "Y 越界: {y}"
        );
    }
}
