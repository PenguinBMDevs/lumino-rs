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

/// 宽度兜底：窗口宽度的绝对下限（逻辑像素）
pub const MIN_DIALOG_WIDTH: f32 = 240.0;

/// 工作区不可用时的宽度测量上限（逻辑像素）
const MAX_MEASURE_WIDTH: f32 = 3840.0;

/// 工作区不可用时的高度测量上限（逻辑像素）
///
/// 防御性：避免文本折行宽度被撑到无意义的大值。
const MAX_MEASURE_HEIGHT: f32 = 2160.0;

/// 单个对话框的尺寸声明
///
/// 宽度是**区间**（`[width_floor, width_cap]`）而不是精确值：区间内由
/// `measure_min_fitting_width` 自动量出「内容不横向溢出的最小宽度」，
/// 声明值只负责兜住两头——`Fill` 占多数内容的下界（自由参数，测量无从判断）
/// 与长文本需要的折行上界。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogSizeHint {
    /// 宽度下限（`0.0` = 完全自动：内容多宽就多宽）
    pub width_floor: f32,
    /// 宽度上限（`None` = 只受工作区限制）
    pub width_cap: Option<f32>,
    /// 高度下限（`None` = 只用 [`MIN_DIALOG_HEIGHT`] 兜底）
    pub min_height: Option<f32>,
    /// 高度上限（`None` = 只受工作区限制）
    pub max_height: Option<f32>,
}

impl DialogSizeHint {
    /// 宽度**完全自动**、高度由测量驱动（内容全部 Fixed 驱动时才可用）
    ///
    /// 实测可自动的 4 类：自定义精度 / 变速 / 批量编辑 / 保存确认。
    const fn auto() -> Self {
        Self {
            width_floor: 0.0,
            width_cap: None,
            min_height: None,
            max_height: None,
        }
    }

    /// 宽度**只在原值基础上增长**（下限 = 原值，上限放到工作区）
    ///
    /// 用于内容宽度可测（表格列宽等 Fixed 内容）、但换行宽度需要兜底的对话框。
    const fn at_least_width(width_floor: f32) -> Self {
        Self {
            width_floor,
            width_cap: None,
            min_height: None,
            max_height: None,
        }
    }

    /// 宽度**钉在原值区间**（下限 = 上限 = 原值）
    ///
    /// 用于 `Fill` 占多数内容（不会被挤溢出 ⇒ 自动宽度会一路退到最小值，
    /// 得到过窄的窗口），或长文本需要固定折行宽度的对话框。
    const fn boxed(width: f32) -> Self {
        Self {
            width_floor: width,
            width_cap: Some(width),
            min_height: None,
            max_height: None,
        }
    }

    /// 钉死高度（`min = max`）：内容含**测量不可信**的滚动区时使用
    const fn pinned(width: f32, height: f32) -> Self {
        Self {
            width_floor: width,
            width_cap: Some(width),
            min_height: Some(height),
            max_height: Some(height),
        }
    }

    /// 钉宽度 + 高度下限（内容随状态变化，开局态未必最高）
    const fn floored(width: f32, min_height: f32) -> Self {
        Self {
            width_floor: width,
            width_cap: Some(width),
            min_height: Some(min_height),
            max_height: None,
        }
    }

    /// 是否需要跑宽度搜索（下限 < 上限时才有区间可搜）
    pub fn needs_width_search(&self, work_area_width: f32) -> bool {
        let cap = self
            .width_cap
            .map_or(work_area_width, |cap| cap.min(work_area_width));

        cap > self.width_floor + 1.0
    }
}

/// 宽度上限（逻辑像素）：声明上限 ∩ 工作区
///
/// 返回 `<= 0` 表示该类型不产生对话框窗口（`DialogType::None`），
/// 调用方须回退种子尺寸。
pub fn width_cap(hint: DialogSizeHint, work_area: Size) -> f32 {
    let work_area_width = if work_area.width.is_finite() && work_area.width > 0.0 {
        work_area.width
    } else {
        MAX_MEASURE_WIDTH
    };

    hint.width_cap
        .map_or(work_area_width, |cap| cap.min(work_area_width))
}

/// 宽度下限（逻辑像素）：声明下限与绝对下限取大，且不超过上限
pub fn width_floor(hint: DialogSizeHint, work_area: Size) -> f32 {
    let cap = width_cap(hint, work_area);

    hint.width_floor.min(cap).max(MIN_DIALOG_WIDTH.min(cap))
}

/// 取某对话框类型的尺寸声明
///
/// `DialogType::None` 返回 0 宽度（该类型不产生对话框窗口），调用方
/// （`Host::measure_dialog_window_size`）据此回退种子尺寸。
pub fn dialog_size_hint(dialog_type: DialogType) -> DialogSizeHint {
    match dialog_type {
        // `None` 只在对话框关闭序列中出现（窗口即将销毁），不参与定尺寸。
        DialogType::None => DialogSizeHint::boxed(0.0),

        // ── 宽度完全自动：内容全部由 Fixed 尺寸驱动 ────────────────────
        // 这些视图里没有 `Fill` 宽度的占位控件，宽度只由 Fixed 输入框 / 下拉 /
        // 按钮 / 标签决定 ⇒ `measure_min_fitting_width` 量出的就是内容真实所需宽度。
        // 自定义精度：Form 面板全 Fixed（旧值 480；实测内容需要 ≈488，旧值会裁 8px）
        DialogType::CustomPrecision => DialogSizeHint::auto(),
        // 变速：内容全 Fixed（旧 400×250）
        DialogType::SpeedChange => DialogSizeHint::auto(),
        // 批量编辑：内容全 Fixed（旧 420×640）
        DialogType::BatchEdit => DialogSizeHint::auto(),
        // 保存确认：三个 90px 按钮 + 文案（旧 380×180；实测内容只需 ≈338）
        DialogType::SaveConfirm => DialogSizeHint::auto(),

        // ── 宽度只在下限基础上增长：内容宽度可测，但换行宽度需兜底 ────────
        // 找回删除音轨：表格列全 Fixed（240+180+80）⇒ 所需宽度可测；
        // 列变宽时窗口自动跟着变宽（旧值 560 作为下限）
        DialogType::RecoverTrack => DialogSizeHint::at_least_width(560.0),
        // 音频导出进度：**当前 Runner 侧无打开路径**（死视图），仅为穷尽性保留
        DialogType::ExportProgress => DialogSizeHint::at_least_width(400.0),

        // ── 宽度钉在原值区间：`Fill` 占多数内容（自动量会退到最小值）──────
        // 加载确认：超长路径是不可断词 token，自动宽度会被它撑爆 ⇒ 钉住并按旧值折行
        DialogType::LoadConfirm => DialogSizeHint::boxed(420.0),
        // 云提醒：纯文本驱动，需固定折行宽度（旧 440×200）
        DialogType::CloudNotice => DialogSizeHint::boxed(440.0),
        // 云连接：6 个 `Fill` 输入框 ⇒ 宽度是自由参数（旧 480×515）
        DialogType::CloudConnect => DialogSizeHint::boxed(480.0),
        // 内存监控：长说明句需固定折行宽度（旧 300×440）
        DialogType::MemoryMonitor => DialogSizeHint::boxed(300.0),
        // 音符画设置：长占位文案需固定折行宽度（旧 460×420）
        DialogType::DrawSettings => DialogSizeHint::boxed(460.0),
        // 帮助：左侧列表 + 右侧滚动正文，尺寸固定，避免测量被滚动区压成 0
        DialogType::Help => DialogSizeHint::pinned(900.0, 620.0),

        // ── 含「测量不可信」的滚动区：钉死高度（min = max） ──────────────
        //
        // 为什么不能交给测量：这些视图把**布局交给滚动区自己决定**（`scrollable`
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

/// 由「宽度 + 测量高度 + 声明 + 工作区」算出最终窗口尺寸（纯函数）
///
/// 宽度已由调用方算好（`measure_min_fitting_width` 的自动结果 + 声明边界），
/// 这里只负责高度：
/// - `min_height == max_height`（[`DialogSizeHint::pinned`]）：**直接用声明值**，
///   不依赖测量（测量失败也不回退种子尺寸）；
/// - 否则 `高 = clamp(measured_height, min_height ?? MIN_DIALOG_HEIGHT, measurement_cap)`。
///
/// 返回 `Size::ZERO` 表示**测量不可用**（宽度非法，或非钉死对话框的高度测量
/// 非正/非有限）：调用方必须回退种子尺寸，绝不允许据此开出 0 尺寸窗口。
pub fn recommend_window_size(
    hint: DialogSizeHint,
    work_area: Size,
    width: f32,
    measured_height: f32,
) -> Size {
    if !width.is_finite() || width <= 0.0 {
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
        return Size::new(width, height.min(work_area_height).max(MIN_DIALOG_HEIGHT));
    }

    if !measured_height.is_finite() || measured_height <= 0.0 {
        return Size::ZERO;
    }

    let max_height = measurement_cap(hint, work_area);
    let floor = hint.min_height.unwrap_or(MIN_DIALOG_HEIGHT);

    // 下限可能高于上限（声明矛盾）：先取 `max(floor)` 再 `min(max_height)` 会让
    // 结果低于下限，故最后再用绝对下限兜一次，保证窗口始终可点击。
    let height = measured_height
        .max(floor)
        .min(max_height)
        .max(MIN_DIALOG_HEIGHT);

    Size::new(width, height)
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

    /// 内容比上限矮 ⇒ 窗口贴合内容（本卡的核心收益）。
    #[test]
    fn test_recommend_hugs_short_content() {
        let hint = dialog_size_hint(DialogType::CustomPrecision);
        let size = recommend_window_size(hint, WORK_AREA, 488.0, 180.0);

        assert_eq!(size.width, 488.0, "宽度取调用方算好的值");
        assert_eq!(
            size.height, 180.0,
            "内容 180 高时应贴合 180，而不是旧的硬编码值"
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
        let hint = dialog_size_hint(DialogType::MemoryMonitor);
        let size = recommend_window_size(hint, WORK_AREA, 300.0, 5000.0);

        assert_eq!(size.height, WORK_AREA.height);
    }

    /// 测量不可用（0 / NaN / 负 / ∞）必须返回 `ZERO`，让调用方回退种子尺寸。
    #[test]
    fn test_recommend_signals_unusable_measurement() {
        let hint = dialog_size_hint(DialogType::SaveConfirm);

        for measured_height in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                recommend_window_size(hint, WORK_AREA, 338.0, measured_height),
                Size::ZERO,
                "高度 {measured_height} 应判为不可用"
            );
        }

        // 宽度非法（0 / NaN / 负）同样不可用
        for width in [0.0_f32, -1.0, f32::NAN] {
            assert_eq!(
                recommend_window_size(hint, WORK_AREA, width, 180.0),
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

    /// 只有「宽度区间有宽度」的对话框才需要跑搜索（省掉 9 次布局）。
    #[test]
    fn test_width_search_only_when_interval_is_wide() {
        // `boxed` / `pinned` / `floored`：区间退化为一点 ⇒ 不需要搜索
        for dialog_type in [
            DialogType::LoadConfirm,
            DialogType::CloudConnect,
            DialogType::MemoryMonitor,
            DialogType::DrawSettings,
            DialogType::Settings,
            DialogType::ProjectSettings,
            DialogType::CloudBrowser,
            DialogType::BrushSettings,
            DialogType::Collaboration,
            DialogType::VideoExport,
        ] {
            assert!(
                !dialog_size_hint(dialog_type).needs_width_search(WORK_AREA.width),
                "{dialog_type:?} 的宽度区间已退化为一点，不应触发搜索"
            );
        }

        // 完全自动 / 只保留下限：需要搜索
        for dialog_type in [
            DialogType::CustomPrecision,
            DialogType::SpeedChange,
            DialogType::BatchEdit,
            DialogType::SaveConfirm,
            DialogType::RecoverTrack,
        ] {
            assert!(
                dialog_size_hint(dialog_type).needs_width_search(WORK_AREA.width),
                "{dialog_type:?} 的宽度交给了自动计算，应触发搜索"
            );
        }
    }
}
