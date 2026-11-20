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
//!    （旧布局引擎）⇒ 长文案"证明"自己需要整个 cap。
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

use lumino_ui_core::geom::Size;

use crate::state::root_state::DialogType;

/// 尺寸兜底：窗口高度的绝对下限（逻辑像素）
///
/// 防御性下限：声明缺失或内容极扁时，窗口仍需可点击（标题栏 + 一行按钮）。
pub const MIN_DIALOG_HEIGHT: f32 = 120.0;

/// 宽度兜底：窗口宽度的绝对下限（逻辑像素）
#[cfg(test)]
const MIN_DIALOG_WIDTH: f32 = 240.0;

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
    ///
    /// 仅测试保留：静态尺寸直接取 [`width_cap`]，不再做二分搜索。
    #[cfg(test)]
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
///
/// 仅测试保留：静态尺寸从 [`width_cap`] 出发，不再需要下限区间。
#[cfg(test)]
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
        DialogType::CustomPrecision => DialogSizeHint::pinned(489.0, 190.0),
        // 变速：内容全 Fixed（旧 400×250）
        DialogType::SpeedChange => DialogSizeHint::pinned(247.0, 256.0),
        // 素材删除确认：标题 / 素材名 / 危险提示 / 两个按钮（DRAW-045 独立窗口）
        DialogType::MaterialDelete => DialogSizeHint::pinned(440.0, 232.0),
        // 自定义 BPM 上限：标签 + 输入框 + 提示 + 错误行 + 两个按钮（DRAW-045）
        DialogType::TempoMaxBpm => DialogSizeHint::pinned(300.0, 268.0),
        // 批量编辑：内容全 Fixed（旧 420×640）
        DialogType::BatchEdit => DialogSizeHint::pinned(319.0, 614.0),
        // 保存确认：尺寸静态化（原 `auto()` 依赖 上一代 视图测量，视图已删除）。
        // 取旧产品尺寸 380×180（实测内容下限 338×169，留出 egui 卡片内边距余量）。
        DialogType::SaveConfirm => DialogSizeHint::pinned(380.0, 180.0),

        // 自动保存恢复（DEBT-12）：完整文件路径是不可断词 token，自动宽度会被
        // 长路径撑爆 ⇒ 钉住宽度并按文案折行（与「加载确认」同类处理）。
        DialogType::AutosaveRecover => DialogSizeHint::boxed(520.0),

        // ── 宽度只在下限基础上增长：内容宽度可测，但换行宽度需兜底 ────────
        // 找回删除音轨：表格列全 Fixed（240+180+80）⇒ 所需宽度可测；
        // 列变宽时窗口自动跟着变宽（旧值 560 作为下限）
        DialogType::RecoverTrack => DialogSizeHint::pinned(565.0, 655.0),
        // 音频导出进度：**当前 Runner 侧无打开路径**（死视图），仅为穷尽性保留
        DialogType::ExportProgress => DialogSizeHint::pinned(406.0, 233.0),

        // ── 宽度钉在原值区间：`Fill` 占多数内容（自动量会退到最小值）──────
        // 加载确认：超长路径是不可断词 token，宽度钉住 420；高度实测 233。
        // （原来高度交给 上一代 视图测量，视图删除后必须静态化）
        DialogType::LoadConfirm => DialogSizeHint::pinned(420.0, 233.0),
        // 云提醒：纯文本驱动，需固定折行宽度（旧 440×200）
        DialogType::CloudNotice => DialogSizeHint::pinned(440.0, 182.0),
        // 云连接：6 个 `Fill` 输入框 ⇒ 宽度是自由参数（旧 480×515）
        DialogType::CloudConnect => DialogSizeHint::pinned(480.0, 476.0),
        // 内存监控：长说明句需固定折行宽度。高度**由内容布局推导**：
        // `PREFERRED_CLIENT_HEIGHT` = 3 行指标 + 6 行子系统（最坏情况）
        // + 3 行折行说明 + 底部按钮带，见 `lumino_ui_egui::memory_monitor`。
        // 旧值 402 是短说明时代的测量结果，装不下 3 行子系统就会压住按钮。
        DialogType::MemoryMonitor => DialogSizeHint::pinned(
            300.0,
            lumino_ui_egui::memory_monitor::PREFERRED_CLIENT_HEIGHT,
        ),
        // 音符画设置：长占位文案需固定折行宽度（旧 460×420）
        DialogType::DrawSettings => DialogSizeHint::pinned(460.0, 172.0),
        // 帮助：左侧列表 + 右侧滚动正文，尺寸固定，避免测量被滚动区压成 0
        DialogType::Help => DialogSizeHint::pinned(900.0, 620.0),
        // 关于：左侧 logo + 右侧品牌行 / 版本号，内容固定
        DialogType::About => DialogSizeHint::pinned(520.0, 220.0),
        // 形状抽屉：B0 空窗先钉死 1000×700；B1 画板接入后再评估测量
        DialogType::ShapeDrawer => DialogSizeHint::pinned(1000.0, 700.0),
        // 撤销历史：固定列表面板，避免历史为空时测量把窗口压成最小高度
        DialogType::History => DialogSizeHint::pinned(440.0, 560.0),
        // 混音台：普通通道条 + 右侧主控条（无内容标题行，30px 标题栏 + 8px 内边距）
        DialogType::Mixer => DialogSizeHint::pinned(760.0, 226.0),

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
        DialogType::Collaboration => DialogSizeHint::pinned(800.0, 600.0),
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

/// 由「声明 + 工作区」直接算出窗口尺寸（**不再测量内容**）
///
/// 上一代 视图层删除后没有可靠的文本度量通道，尺寸必须完全由声明决定：
/// - 宽度 = [`width_cap`]（所有对话框的宽度都是固定声明值）；
/// - 高度 = `min_height`（`pinned` / `floored` 都声明了下限），再按
///   [`measurement_cap`] 与工作区夹取，并保证不低于 [`MIN_DIALOG_HEIGHT`]。
///
/// 返回 `Size::ZERO` 表示声明非法（宽度非有限/非正）：调用方回退种子尺寸。
pub fn static_window_size(hint: DialogSizeHint, work_area: Size) -> Size {
    let width = width_cap(hint, work_area);
    if !width.is_finite() || width <= 0.0 {
        return Size::ZERO;
    }

    let work_area_height = if work_area.height.is_finite() && work_area.height > 0.0 {
        work_area.height
    } else {
        MAX_MEASURE_HEIGHT
    };

    // `pinned` 与 `floored` 都声明了高度；未声明时用绝对下限。
    let declared = hint.min_height.unwrap_or(MIN_DIALOG_HEIGHT);
    let height = declared
        .min(measurement_cap(hint, work_area))
        .max(MIN_DIALOG_HEIGHT)
        .min(work_area_height);

    Size::new(width, height)
}

#[cfg(test)]
/// 由「宽度 + 测量高度 + 声明 + 工作区」算出最终窗口尺寸（纯函数）
///
/// **仅测试保留**：生产路径已改为 [`static_window_size`]（无内容测量）。
/// 本函数用于锁定「钉死高度不受测量影响」这一声明语义。
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
mod tests;
