//! 对话框尺寸声明表的回归测试（从 dialog_preferred_size.rs 拆出，保持单文件 ≤600 行）。

use super::*;

/// 全部「真实」对话框类型（`None` 除外）
const ALL_DIALOGS: [DialogType; 25] = [
    DialogType::CustomPrecision,
    DialogType::Collaboration,
    DialogType::LoadConfirm,
    DialogType::ProjectSettings,
    DialogType::Settings,
    DialogType::SpeedChange,
    DialogType::MaterialDelete,
    DialogType::TempoMaxBpm,
    DialogType::BatchEdit,
    DialogType::ExportProgress,
    DialogType::VideoExport,
    DialogType::MemoryMonitor,
    DialogType::RecoverTrack,
    DialogType::SaveConfirm,
    DialogType::AutosaveRecover,
    DialogType::CloudConnect,
    DialogType::CloudBrowser,
    DialogType::CloudNotice,
    DialogType::BrushSettings,
    DialogType::DrawSettings,
    DialogType::Help,
    DialogType::About,
    DialogType::ShapeDrawer,
    DialogType::History,
    DialogType::Mixer,
];

/// 每个对话框的宽度区间都必须可用。
///
/// match 的穷尽性由编译器保证（新增类型不声明就编译失败）；本测试再挡一层
/// 「声明了 0 / 负数 / NaN / 区间空」——那会让测量入口判非法并回退种子尺寸，
/// 等于悄悄放弃按内容定尺寸。
#[test]
fn test_all_dialog_types_declare_usable_width() {
    for dialog_type in ALL_DIALOGS {
        let hint = dialog_size_hint(dialog_type);
        let cap = width_cap(hint, WORK_AREA);
        let floor = width_floor(hint, WORK_AREA);

        assert!(
            cap.is_finite() && cap > 0.0,
            "{dialog_type:?} 的宽度上限无效: {cap}（必须有限且为正）"
        );
        assert!(
            floor.is_finite() && floor > 0.0,
            "{dialog_type:?} 的宽度下限无效: {floor}"
        );
        assert!(
            floor <= cap,
            "{dialog_type:?} 的宽度区间为空: [{floor}, {cap}]"
        );
    }
}

/// 上下限必须自洽：声明了上限就不得低于下限，也不得低于绝对下限。
#[test]
fn test_declared_bounds_are_consistent() {
    for dialog_type in ALL_DIALOGS {
        let hint = dialog_size_hint(dialog_type);

        if let Some(max_height) = hint.max_height {
            assert!(
                max_height.is_finite() && max_height >= MIN_DIALOG_HEIGHT,
                "{dialog_type:?} 的高度上限 {} 不合理（应 ≥ {MIN_DIALOG_HEIGHT}）",
                max_height
            );
        }

        assert!(
            !(hint.min_height.is_some()
                && hint.max_height.is_some()
                && hint.min_height > hint.max_height),
            "{dialog_type:?} 的高度下限高于上限，区间为空"
        );
    }
}

/// 内容含**测量不可信**的滚动区的对话框，必须钉死高度
/// （`min_height = max_height`）——否则压缩测量会把滚动区外层压成 0，
/// 窗口高度按残缺内容算出（实测设置面板会短掉整块菜单 524px）。
#[test]
fn test_measurement_hostile_dialogs_are_pinned() {
    for dialog_type in [
        DialogType::Settings,
        DialogType::ProjectSettings,
        DialogType::CloudBrowser,
        DialogType::BrushSettings,
    ] {
        let hint = dialog_size_hint(dialog_type);
        assert_eq!(
            hint.min_height, hint.max_height,
            "{dialog_type:?} 的滚动区测量不可信，必须用 pinned（min = max）钉死高度"
        );
        assert!(
            hint.max_height.is_some(),
            "{dialog_type:?} 必须声明高度（不能交给测量）"
        );
    }
}

/// 钉死的对话框：无论测量结果偏大偏小，窗口高度恒为声明值。
#[test]
fn test_pinned_dialogs_ignore_measurement_for_height() {
    let hint = dialog_size_hint(DialogType::Settings);
    let width = 720.0;

    for measured_height in [0.0_f32, 152.0, 540.0, 4000.0] {
        let size = recommend_window_size(hint, WORK_AREA, width, measured_height);
        assert_eq!(
            size.height, 540.0,
            "测量 {measured_height} 时窗口高度应恒为钉死值 540"
        );
    }
}

/// `None` 不产生对话框窗口：宽度上限为 0（调用方据此回退种子尺寸）。
#[test]
fn test_none_has_no_usable_width() {
    let hint = dialog_size_hint(DialogType::None);
    assert_eq!(width_cap(hint, WORK_AREA), 0.0);
    assert_eq!(width_floor(hint, WORK_AREA), 0.0);
}

const WORK_AREA: Size = Size::new(1920.0, 1040.0);

/// 内容高于下限但未触顶 ⇒ 窗口贴合内容（本卡的核心收益）。
#[test]
fn test_recommend_hugs_short_content() {
    // VideoExport 是仅剩的「高度受测量驱动」对话框（floored：下限 560、无上限）。
    let hint = dialog_size_hint(DialogType::VideoExport);
    let size = recommend_window_size(hint, WORK_AREA, 520.0, 700.0);

    assert_eq!(size.width, 520.0, "宽度取调用方算好的值");
    assert_eq!(
        size.height, 700.0,
        "内容 700 高时应贴合 700（高于下限 560）"
    );
}

/// 内容超过声明上限（无界滚动区）⇒ 夹到上限，交给滚动区消化。
#[test]
fn test_recommend_clamps_scroll_content_to_declared_ceiling() {
    let hint = dialog_size_hint(DialogType::Settings);
    let size = recommend_window_size(hint, WORK_AREA, 720.0, 2000.0);

    assert_eq!(size.height, 540.0, "设置页内容再长也不得超过声明上限");
}

/// 内容比下限矮 ⇒ 抬到下限（内容随状态变化的对话框防裁剪）。
#[test]
fn test_recommend_applies_floor() {
    let hint = dialog_size_hint(DialogType::VideoExport);
    let size = recommend_window_size(hint, WORK_AREA, 520.0, 300.0);

    assert_eq!(
        size.height, 560.0,
        "视频导出需按旧高度兜底，避免切到高状态后裁剪"
    );
}

/// 未声明上限的对话框仍受**工作区**约束（不超可见区域）。
#[test]
fn test_recommend_never_exceeds_work_area() {
    // VideoExport 无 `max_height`：内容再长也由工作区兜住。
    let hint = dialog_size_hint(DialogType::VideoExport);
    let size = recommend_window_size(hint, WORK_AREA, 520.0, 5000.0);

    assert_eq!(size.height, WORK_AREA.height);
}

/// 测量不可用（0 / NaN / 负 / ∞）必须返回 `ZERO`，让调用方回退种子尺寸。
#[test]
fn test_recommend_signals_unusable_measurement() {
    // VideoExport（floored）仍走测量；钉死的对话框不受测量影响。
    let hint = dialog_size_hint(DialogType::VideoExport);

    for measured_height in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            recommend_window_size(hint, WORK_AREA, 520.0, measured_height),
            Size::ZERO,
            "高度 {measured_height} 应判为不可用"
        );
    }

    // 宽度非法（0 / NaN / 负）同样不可用
    for width in [0.0_f32, -1.0, f32::NAN] {
        assert_eq!(
            recommend_window_size(hint, WORK_AREA, width, 700.0),
            Size::ZERO,
            "宽度 {width} 应判为不可用"
        );
    }
}

/// 工作区不可用（`ZERO` / NaN）时不得把上限算成 0——那会开出 0 尺寸窗口。
#[test]
fn test_recommend_survives_missing_work_area() {
    let hint = dialog_size_hint(DialogType::SaveConfirm);

    for work_area in [Size::ZERO, Size::new(0.0, f32::NAN)] {
        let cap = width_cap(hint, work_area);
        assert!(cap > 0.0, "工作区不可用时宽度上限仍须为正，实测 {cap}");

        let size = recommend_window_size(hint, work_area, cap, 180.0);
        assert!(
            size.height >= MIN_DIALOG_HEIGHT,
            "工作区不可用时高度仍须可点击，实测 {}",
            size.height
        );
    }
}

/// 测量上限与实际夹取必须同源：否则内容会在夹取步骤被裁掉。
#[test]
fn test_measurement_cap_is_the_same_bound_used_for_clamping() {
    for dialog_type in ALL_DIALOGS {
        let hint = dialog_size_hint(dialog_type);
        let cap = measurement_cap(hint, WORK_AREA);
        let width = width_floor(hint, WORK_AREA);

        // 恰好等于上限的测量结果不得被夹小
        let size = recommend_window_size(hint, WORK_AREA, width, cap);
        assert_eq!(
            size.height, cap,
            "{dialog_type:?}：上限 {cap} 处的测量结果被夹成 {}，测量与夹取口径不一致",
            size.height
        );
    }
}

/// 所有对话框的宽度区间都已按实测尺寸钉死 ⇒ 不再触发宽度搜索。
#[test]
fn test_width_search_only_when_interval_is_wide() {
    for dialog_type in ALL_DIALOGS {
        assert!(
            !dialog_size_hint(dialog_type).needs_width_search(WORK_AREA.width),
            "{dialog_type:?} 的宽度区间已退化为一点，不应触发搜索"
        );
    }
}

/// 静态尺寸：每一类对话框都给出可用的逻辑尺寸（宽 = 声明值，高在工作区内）。
#[test]
fn test_static_window_size_is_usable_for_every_dialog() {
    for dialog_type in ALL_DIALOGS {
        let hint = dialog_size_hint(dialog_type);
        let size = static_window_size(hint, WORK_AREA);
        assert!(
            size.width.is_finite() && size.width > 0.0,
            "{dialog_type:?} 宽度无效: {}",
            size.width
        );
        assert!(
            (size.width - width_cap(hint, WORK_AREA)).abs() < f32::EPSILON,
            "{dialog_type:?} 宽度应取声明值 {}",
            width_cap(hint, WORK_AREA)
        );
        assert!(
            size.height >= MIN_DIALOG_HEIGHT && size.height <= WORK_AREA.height,
            "{dialog_type:?} 高度越界: {}",
            size.height
        );
    }
}

/// 声明高度被原样采用：`pinned` 用 min=max，`floored` 用下限。
#[test]
fn test_static_window_size_uses_declared_height() {
    assert_eq!(
        static_window_size(dialog_size_hint(DialogType::Settings), WORK_AREA).height,
        540.0
    );
    assert_eq!(
        static_window_size(dialog_size_hint(DialogType::VideoExport), WORK_AREA).height,
        560.0,
        "floored 只声明下限 ⇒ 静态尺寸取下限"
    );
    assert_eq!(
        static_window_size(dialog_size_hint(DialogType::None), WORK_AREA),
        Size::ZERO,
        "None 不产生对话框窗口"
    );
}

/// 内存监控的窗口高度必须引用内容布局常量（不得再写死数字）：
/// 该对话框内容随子系统行数增长，旧值 402 会被 3 行子系统压住按钮。
#[test]
fn test_memory_monitor_height_comes_from_content_layout() {
    assert_eq!(
        dialog_size_hint(DialogType::MemoryMonitor).min_height,
        Some(lumino_ui_egui::memory_monitor::PREFERRED_CLIENT_HEIGHT),
        "内存监控窗口高度必须由 ui-egui 的内容布局常量推导"
    );
}

/// 工作区比声明高度还矮时，高度被夹到工作区（不越屏）。
#[test]
fn test_static_window_size_clamps_to_work_area() {
    let small = Size::new(1920.0, 300.0);
    let size = static_window_size(dialog_size_hint(DialogType::Settings), small);
    assert_eq!(size.height, 300.0);
}
