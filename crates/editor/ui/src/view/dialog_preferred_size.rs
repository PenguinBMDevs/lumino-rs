//! REF-001：对话框**尺寸声明表**（决策 D3：宽由视图声明、高由测量驱动）
//!
//! # 最终尺寸规则
//!
//! ```text
//! 宽  = hint.width                                   // 必须声明
//! 高  = clamp(测量高度, hint.min_height, hint.max_height, 工作区上限)
//! ```
//!
//! 三条依据（详见 `docs/2026-10-16-REF-001-统一弹窗窗口尺寸决策调研.md`）：
//!
//! 1. **宽度不可测量驱动**：`TextInput`/`ProgressBar` 的内在尺寸恒为
//!    `Size::ZERO`（`text_input.rs` 的 `resolve(..., Size::ZERO)`、
//!    `progress_bar.rs` 的 `layout::atomic`）⇒ `Fill` 宽度量出 0；
//!    `Text` 又以 `limits.max()` 为折行宽再取 `min_bounds`
//!    （`iced_core/src/widget/text.rs`）⇒ 长文案"证明"自己需要整个 cap。
//!    所以宽度必须声明，它同时是文本的折行宽度。
//! 2. **高度可测量驱动**：压缩限制下 `Fill` 退化为内容内在尺寸，且结果天然被
//!    `max_height` 夹住。
//! 3. **高度只在可信时交给测量**：
//!    - `Measured`（`min_height`/`max_height` 均为 `None`）：内容有界、布局由
//!      内容自身决定 ⇒ 高度测量驱动（这是本卡的收益）。
//!    - `min_height` 必填：内容**随状态变化**且开局态未必最高（协作 / 视频导出的
//!      多状态），用已知可用值兜底。
//!    - `min_height = max_height`（[`DialogSizeHint::pinned`]）：内容含
//!      **测量不可信**的滚动区（滚动区自己决定布局，压缩测量时其外层会被压成 0），
//!      高度直接按产品决策钉死。
//!
//! # 数值来源
//!
//! 取原 `lumino-dialog` 的 `DialogWindow::new` per-type 硬编码尺寸
//! （REF-001 阶段 C 前位于 `crates/editor/dialog/src/window.rs:53-72`）：
//! - `width` 取原宽度 ⇒ **宽度视觉零回归**；
//! - `max_height` / `min_height` 取原高度 ⇒ 原来是"精确尺寸"，现在退化为
//!   「上限/下限」，内容合适时窗口会自动贴合内容（这是本卡要的收益）。
//!
//! 新增 `DialogType` 时本表会**编译失败**——偏好宽度是必填项，不允许退回
//! 「测量一个会自证的宽度」。

use iced_core::Size;

use crate::state::root_state::DialogType;

/// 尺寸兜底：窗口高度的绝对下限（逻辑像素）
///
/// 防御性下限：测量不可用或内容极扁时，窗口仍需可点击（标题栏 + 一行按钮）。
pub const MIN_DIALOG_HEIGHT: f32 = 120.0;

/// 工作区不可用时的高度测量上限（逻辑像素）
///
/// 防御性：避免文本折行宽度被撑到无意义的大值。
const MAX_MEASURE_HEIGHT: f32 = 2160.0;

/// 单个对话框的尺寸声明（宽度必填，上下限按需）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogSizeHint {
    /// 偏好排版宽度（逻辑像素）：既是窗口宽度，也是文本折行宽度
    pub width: f32,
    /// 高度下限（`None` = 只用 [`MIN_DIALOG_HEIGHT`] 兜底）
    pub min_height: Option<f32>,
    /// 高度上限（`None` = 只受工作区限制；含无界滚动区时必填）
    pub max_height: Option<f32>,
}

impl DialogSizeHint {
    /// 只声明宽度（高度完全由测量驱动）
    const fn width_only(width: f32) -> Self {
        Self {
            width,
            min_height: None,
            max_height: None,
        }
    }

    /// 钉死高度（`min = max`）：内容含**测量不可信**的滚动区时使用
    ///
    /// 见 [`pinned`] 的说明：这类对话框的高度是产品决策，不能交给测量。
    const fn pinned(width: f32, height: f32) -> Self {
        Self {
            width,
            min_height: Some(height),
            max_height: Some(height),
        }
    }

    /// 声明宽度 + 高度下限（内容随状态变化，开局态未必最高）
    const fn floored(width: f32, min_height: f32) -> Self {
        Self {
            width,
            min_height: Some(min_height),
            max_height: None,
        }
    }
}

/// 取某对话框类型的尺寸声明
///
/// `DialogType::None` 返回全 0 宽度（该类型不产生对话框窗口），调用方
/// （`Host::measure_dialog_window_size`）据此回退种子尺寸。
pub fn dialog_size_hint(dialog_type: DialogType) -> DialogSizeHint {
    match dialog_type {
        // `None` 只在对话框关闭序列中出现（窗口即将销毁），不参与定尺寸。
        DialogType::None => DialogSizeHint::width_only(0.0),

        // ── 静态、无滚动区：高度完全由测量驱动 ────────────────────────
        // 自定义精度：内容为 Fixed 驱动的面板（测量值 ≈488×180，与旧值一致）
        DialogType::CustomPrecision => DialogSizeHint::width_only(480.0),
        // 变速：内容全 Fixed（旧 400×250）
        DialogType::SpeedChange => DialogSizeHint::width_only(400.0),
        // 批量编辑：内容全 Fixed（旧 420×640）
        DialogType::BatchEdit => DialogSizeHint::width_only(420.0),
        // 保存确认：内容静态（旧 380×180）
        DialogType::SaveConfirm => DialogSizeHint::width_only(380.0),
        // 加载确认：内容静态；超长路径在声明宽度内折行（旧 420×260）
        DialogType::LoadConfirm => DialogSizeHint::width_only(420.0),
        // 云提醒：纯文本驱动（旧 440×200）
        DialogType::CloudNotice => DialogSizeHint::width_only(440.0),
        // 云连接：表单高度固定（旧 480×515）
        DialogType::CloudConnect => DialogSizeHint::width_only(480.0),
        // 内存监控：指标集固定，内容高度稳定（旧 300×440）
        DialogType::MemoryMonitor => DialogSizeHint::width_only(300.0),
        // 音符画设置：占位面板，撑位已改固定间距（旧 460×420，占位期留白将消失）
        DialogType::DrawSettings => DialogSizeHint::width_only(460.0),
        // 音频导出进度：**当前 Runner 侧无打开路径**（死视图），仅为穷尽性保留
        DialogType::ExportProgress => DialogSizeHint::width_only(400.0),

        // ── 含「测量不可信」的滚动区：钉死高度（min = max） ──────────────
        //
        // 为什么不能交给测量：这些视图把**布局可由滚动区自己决定**（`scrollable`
        // 高度 Fill，滚动轴按内容全长布局）。压缩测量时外层会被压成 0 而内容仍在，
        // 实测（阶段 F dump，工作区 1920×1040、排版宽度取声明值）：
        // - 设置：菜单列表内容真实高 524，测量时外层 200×0 ⇒ 整块菜单没被计入；
        // - 云文件浏览 / 画刷设置：列表在打开时是空的（连接/音轨随后注入），
        //   测量到的是"空列表"的高度。
        // 因此这四类的高度是**产品决策**（一次展示多高列表），按旧值钉死：
        // min = max 同时表达"不低于此高"与"不高于此高"，渲染期视图仍是纯 Fill，
        // 滚动区随窗口增长。
        // 设置：滚动设置页 → 钉 540
        DialogType::Settings => DialogSizeHint::pinned(720.0, 540.0),
        // 工程设置：滚动表单 → 钉 480
        DialogType::ProjectSettings => DialogSizeHint::pinned(450.0, 480.0),
        // 云文件浏览：文件列表打开时为空 → 钉 520
        DialogType::CloudBrowser => DialogSizeHint::pinned(720.0, 520.0),
        // 画刷设置：音轨列表打开时可能为空 → 钉 600
        DialogType::BrushSettings => DialogSizeHint::pinned(480.0, 600.0),
        // 找回删除音轨：列表区已 `Fixed(LIST_MAX_HEIGHT=400)` 有界（`Fixed`
        // 会复位压缩，测量可信）⇒ 高度由测量驱动（实测 655，旧值 770）。
        // 列表为空时高度也不缩，这是刻意的：条目由 Runner 晚一拍注入。
        DialogType::RecoverTrack => DialogSizeHint::width_only(560.0),

        // ── 内容随状态变化：必须声明高度下限 ──────────────────────────
        // 协作：连接 / 连接中 / 房间操作 / 房间内 四态内容高度不同，
        // 开局态未必最高 ⇒ 用旧高度 600 兜底（内容更高时仍按测量增长）
        DialogType::Collaboration => DialogSizeHint::floored(800.0, 600.0),
        // 视频导出：Exporting / Finalizing / Completed / Error 四态 + 进度刷新，
        // 同样按旧高度 560 兜底
        DialogType::VideoExport => DialogSizeHint::floored(520.0, 560.0),
    }
}

/// 高度测量上限（逻辑像素）：声明上限与工作区取小；两者都没有时用保守大值
///
/// 测量与最终尺寸都必须用同一个上限——否则「测量时按工作区折行、定尺寸时按
/// 声明上限夹取」两步的口径不一致，内容会在夹取那一步被裁掉。
pub fn measurement_cap(hint: DialogSizeHint, work_area: Size) -> f32 {
    let work_area_usable = work_area.height.is_finite() && work_area.height > 0.0;

    match (hint.max_height, work_area_usable) {
        (Some(declared), true) => declared.min(work_area.height),
        (Some(declared), false) => declared,
        (None, true) => work_area.height,
        (None, false) => MAX_MEASURE_HEIGHT,
    }
}

/// 由「测量结果 + 声明 + 工作区」算出最终窗口尺寸（纯函数）
///
/// 规则：`宽 = hint.width`；高度分三种：
/// - `min_height == max_height`（[`DialogSizeHint::pinned`]）：**直接用声明值**，
///   不依赖测量（测量失败也不回退种子尺寸）；
/// - 否则 `高 = clamp(measured, min_height ?? MIN_DIALOG_HEIGHT, measurement_cap)`；
///
/// 返回 `Size::ZERO` 表示**测量不可用**（宽度未声明，或非钉死对话框的测量结果
/// 非正/非有限）：调用方必须回退种子尺寸，绝不允许据此开出 0 尺寸窗口。
pub fn recommend_window_size(hint: DialogSizeHint, work_area: Size, measured: Size) -> Size {
    if !hint.width.is_finite() || hint.width <= 0.0 {
        return Size::ZERO;
    }

    let work_area_height = if work_area.height.is_finite() && work_area.height > 0.0 {
        work_area.height
    } else {
        MAX_MEASURE_HEIGHT
    };

    // 钉死高度（`min = max`）：高度是产品决策，**不依赖测量** ⇒
    // 测量失败（0/NaN）也不该把窗口推回种子尺寸。
    let pinned_height = match (hint.min_height, hint.max_height) {
        (Some(min_height), Some(max_height)) if min_height == max_height => Some(min_height),
        _ => None,
    };
    if let Some(height) = pinned_height {
        return Size::new(
            hint.width,
            height.min(work_area_height).max(MIN_DIALOG_HEIGHT),
        );
    }

    if !measured.height.is_finite() || measured.height <= 0.0 {
        return Size::ZERO;
    }

    let max_height = measurement_cap(hint, work_area);
    let floor = hint.min_height.unwrap_or(MIN_DIALOG_HEIGHT);

    // 下限可能高于上限（声明矛盾）：先取 `max(floor)` 再 `min(max_height)` 会让
    // 结果低于下限，故最后再用绝对下限兜一次，保证窗口始终可点击。
    let height = measured
        .height
        .max(floor)
        .min(max_height)
        .max(MIN_DIALOG_HEIGHT);

    Size::new(hint.width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部「真实」对话框类型（`None` 除外）
    const ALL_DIALOGS: [DialogType; 17] = [
        DialogType::CustomPrecision,
        DialogType::Collaboration,
        DialogType::LoadConfirm,
        DialogType::ProjectSettings,
        DialogType::Settings,
        DialogType::SpeedChange,
        DialogType::BatchEdit,
        DialogType::ExportProgress,
        DialogType::VideoExport,
        DialogType::MemoryMonitor,
        DialogType::RecoverTrack,
        DialogType::SaveConfirm,
        DialogType::CloudConnect,
        DialogType::CloudBrowser,
        DialogType::CloudNotice,
        DialogType::BrushSettings,
        DialogType::DrawSettings,
    ];

    /// 每个对话框都必须声明**正的**偏好宽度。
    ///
    /// match 的穷尽性由编译器保证（新增类型不声明就编译失败）；本测试再挡一层
    /// 「声明了 0 / 负数 / NaN」——那会让测量入口判非法并回退种子尺寸，
    /// 等于悄悄放弃按内容定尺寸。
    #[test]
    fn test_all_dialog_types_declare_usable_width() {
        for dialog_type in ALL_DIALOGS {
            let hint = dialog_size_hint(dialog_type);
            assert!(
                hint.width.is_finite() && hint.width > 0.0,
                "{dialog_type:?} 的偏好宽度无效: {}（必须有限且为正）",
                hint.width
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

        for measured_height in [0.0_f32, 152.0, 540.0, 4000.0] {
            let size =
                recommend_window_size(hint, WORK_AREA, Size::new(hint.width, measured_height));
            assert_eq!(
                size.height, 540.0,
                "测量 {measured_height} 时窗口高度应恒为钉死值 540"
            );
        }
    }

    /// `None` 不产生对话框窗口，宽度声明为 0（调用方据此回退）。
    #[test]
    fn test_none_has_no_preferred_width() {
        assert_eq!(dialog_size_hint(DialogType::None).width, 0.0);
    }

    const WORK_AREA: Size = Size::new(1920.0, 1040.0);

    /// 内容比上限矮 ⇒ 窗口贴合内容（本卡的核心收益）。
    #[test]
    fn test_recommend_hugs_short_content() {
        let hint = dialog_size_hint(DialogType::CustomPrecision);
        let size = recommend_window_size(hint, WORK_AREA, Size::new(480.0, 180.0));

        assert_eq!(size.width, 480.0);
        assert_eq!(
            size.height, 180.0,
            "内容 180 高时应贴合 180，而不是旧的硬编码值"
        );
    }

    /// 内容超过声明上限（无界滚动区）⇒ 夹到上限，交给滚动区消化。
    #[test]
    fn test_recommend_clamps_scroll_content_to_declared_ceiling() {
        let hint = dialog_size_hint(DialogType::Settings);
        let size = recommend_window_size(hint, WORK_AREA, Size::new(720.0, 2000.0));

        assert_eq!(size.height, 540.0, "设置页内容再长也不得超过声明上限");
    }

    /// 内容比下限矮 ⇒ 抬到下限（内容随状态变化的对话框防裁剪）。
    #[test]
    fn test_recommend_applies_floor() {
        let hint = dialog_size_hint(DialogType::VideoExport);
        let size = recommend_window_size(hint, WORK_AREA, Size::new(520.0, 300.0));

        assert_eq!(
            size.height, 560.0,
            "视频导出需按旧高度兜底，避免切到高状态后裁剪"
        );
    }

    /// 未声明上限的对话框仍受**工作区**约束（不超可见区域）。
    #[test]
    fn test_recommend_never_exceeds_work_area() {
        let hint = dialog_size_hint(DialogType::MemoryMonitor);
        let size = recommend_window_size(hint, WORK_AREA, Size::new(300.0, 5000.0));

        assert_eq!(size.height, WORK_AREA.height);
    }

    /// 测量不可用（0 / NaN / 负）必须返回 `ZERO`，让调用方回退种子尺寸。
    #[test]
    fn test_recommend_signals_unusable_measurement() {
        let hint = dialog_size_hint(DialogType::SaveConfirm);

        for measured in [
            Size::new(380.0, 0.0),
            Size::new(380.0, -1.0),
            Size::new(380.0, f32::NAN),
            Size::new(380.0, f32::INFINITY),
        ] {
            assert_eq!(
                recommend_window_size(hint, WORK_AREA, measured),
                Size::ZERO,
                "{measured:?} 应判为不可用"
            );
        }

        // 宽度未声明（None 类型）同样不可用
        assert_eq!(
            recommend_window_size(
                dialog_size_hint(DialogType::None),
                WORK_AREA,
                Size::new(100.0, 100.0)
            ),
            Size::ZERO
        );
    }

    /// 工作区不可用（`ZERO` / NaN）时不得把上限算成 0——那会开出 0 高窗口。
    #[test]
    fn test_recommend_survives_missing_work_area() {
        let hint = dialog_size_hint(DialogType::SaveConfirm);

        for work_area in [Size::ZERO, Size::new(0.0, f32::NAN)] {
            let size = recommend_window_size(hint, work_area, Size::new(380.0, 180.0));
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

            // 恰好等于上限的测量结果不得被夹小
            let size = recommend_window_size(hint, WORK_AREA, Size::new(hint.width, cap));
            assert_eq!(
                size.height, cap,
                "{dialog_type:?}：上限 {cap} 处的测量结果被夹成 {}，测量与夹取口径不一致",
                size.height
            );
        }
    }
}
