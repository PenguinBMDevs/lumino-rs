//! REF-001：对话框内容尺寸测量（**打开时测量一次**，不做运行时持续跟踪）
//!
//! # 为什么需要这个入口
//!
//! 对话框窗口尺寸原先硬编码在 `lumino-dialog` 的 `DialogWindow::new`
//! （17 种 `DialogType` 各一组 `(width, height)`），而对话框视图根一律是
//! `Length::Fill`——「内容改了但数字没回填」就会裁剪或留白。
//!
//! iced 0.14 提供了一条**不需要改任何视图根声明**的测量通道：
//! [`Limits::width`] 传 `Length::Shrink` 时只置 `compression` 标志，
//! [`Limits::resolve`] 随即把 `Fill`/`FillPortion` 退化为内容的**内在尺寸**
//! （`iced_core/src/layout/limits.rs`）。
//!
//! # ⚠️ 调用侧必须遵守的尺寸口径（REF-001 §5.1 决策 D3）
//!
//! - **高度可信**：`Fill` 退化为内在尺寸，且结果天然被 `cap` 钳住
//!   （`resolve` 内部 `.min(max)`），因此「不超工作区」不需要额外 clamp。
//! - **宽度不可信**，两个原因：
//!   1. `TextInput` 的内在尺寸恒为 `Size::ZERO`
//!      （`text_input.rs` 的 `resolve(self.width, height, Size::ZERO)`），
//!      `ProgressBar` 同理（`progress_bar.rs` 的 `layout::atomic`）⇒ `Fill`
//!      宽度会量出 **0**；
//!   2. `Text` 以 `limits.max()` 为折行宽度再取 `min_bounds`
//!      （`iced_core/src/widget/text.rs`）⇒ 长文案量出的宽度**就是传入的 cap**
//!      （自证循环）。
//!
//!   所以宽度侧必须由**视图声明的偏好宽度**决定，调用时把它作为 `cap.width`
//!   传入（它同时充当文本折行宽度），**不要把返回值的宽度当作内容宽度**。
//!   宽度改为自动计算后，声明值退化为区间边界，真正的下界由
//!   [`measure_min_fitting_width`] 二分求出（实现在 [`search`]）。
//! - 含**无界滚动区**的对话框（`scrollable` 的滚动轴取内容全长）在压缩测量下
//!   高度恒等于 `cap`，因此必须在尺寸声明里给出 `max_height`，由
//!   [`Host::measure_dialog_window_size`] 夹住——**不需要改视图**（视图保持
//!   纯 `Fill`，滚动区在用户放大窗口时仍会随之增长）。反例与正例见
//!   [`contract_tests`] 的
//!   `contract_scrollable_without_cap_eats_cap_height` /
//!   `contract_scrollable_with_cap_clamps_to_preferred`。
//! - `space().height(Length::Fill)` 这类**柔性撑位**会在测量期吃掉整个上限并把
//!   其后的兄弟压成 0（契约测试
//!   `contract_trailing_fixed_sibling_after_fill_is_zeroed`）。窗口改为贴合内容
//!   后这类撑位已无意义，视图侧应改用固定间距（见 `view::draw_settings_dialog`）。
//!
//! 返回 `Size::ZERO` 表示**测量不可用**（入参非法，或布局结果非有限/非正）。
//! 调用方必须回退到种子尺寸，**绝不允许因此不显示窗口**。
//!
//! # 模块结构
//!
//! | 文件 | 职责 |
//! |------|------|
//! | `measure`（本文件） | 测量核心（固定宽 + 压缩高）与尺寸决策 |
//! | `measure/search` | 宽度自动计算：二分求「不被压窄的最小宽度」 |
//! | `measure/contract_tests` | iced 0.14 布局语义的契约测试 |
//! | `measure/dialog_sizes` | 17 类尺寸验证（与生产同源、防口径漂移） |
//! | `measure/edge` | 边缘控件可见性尺子（验收门槛） |

use iced_core::layout::Limits;
use iced_core::widget::Tree;
use iced_core::{Length, Size};

use crate::view::dialog_preferred_size;
use crate::{Element, Renderer};

use super::Host;

mod search;

pub use search::measure_min_fitting_width;

// 判据与基准只给测试用（生产路径只调完整的宽度搜索）
#[cfg(test)]
pub use search::{measure_is_squeezed, measure_width_baseline};

/// 尺寸是否可用（非零、有限、为正）
fn usable(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

/// 阻断「文本按 0 宽折行」的测量核心：**固定排版宽度 + 压缩高度**
///
/// # 为什么宽度必须固定，而不能两个轴都压缩
///
/// 直觉做法是给两个轴都传 `Length::Shrink`（让 `Fill` 全部退化为内在尺寸），
/// 但那样**宽度会塌成 0**，进而让文本按 0 宽折行、量出虚高到荒谬的内容高度。
///
/// 机理在 iced 的 flex 布局：当同一层的子节点**横向全是 `Fill`** 时
/// （`view_dialog` 的自定义标题栏包裹层正是 `column![标题栏(Fill), 内容(Fill)]`），
/// `cross` 初值 0 且 pass 1 不会更新它，pass 2 又把 `cross` 当作子节点的
/// `max_width`（`iced_core/src/layout/flex.rs`）⇒ 子树拿到的可用宽度是 0。
/// 实测（阶段 F）：保存确认对话框的提示文字被量成 208px 高、内存监控的说明文字
/// 被量成 337px 高，多个对话框的高度虚高到工作区上限（1040）。
///
/// 因此正确口径是：**宽度 = 视图声明的排版宽度（`min = max`，硬约束）**，
/// **高度 = 压缩**（`Fill` 退化为内在尺寸）。这样文本按声明宽度折行，
/// 高度才是真实内容高度。
///
/// [`Host`] 与单测共用本函数，保证测试口径与生产口径不漂移。
pub fn measure_content_height(
    view: Element<'_>,
    renderer: &Renderer,
    layout_width: f32,
    max_height: f32,
) -> f32 {
    puffin::profile_scope!("dialog_measure_content");

    let Some(node) = layout_at_width(view, renderer, layout_width, max_height) else {
        return 0.0;
    };

    let size = node.size();
    if !size.height.is_finite() || size.height <= 0.0 {
        return 0.0;
    }

    size.height
}

/// 在「固定宽度 + 压缩高度」下跑一次布局，返回根节点
fn layout_at_width(
    view: Element<'_>,
    renderer: &Renderer,
    layout_width: f32,
    max_height: f32,
) -> Option<iced_core::layout::Node> {
    if !layout_width.is_finite()
        || layout_width <= 0.0
        || !max_height.is_finite()
        || max_height <= 0.0
    {
        return None;
    }

    // 与渲染路径同源的视图树；`Tree` 新建后即丢弃 ⇒ 对渲染的控件状态零副作用。
    let mut element = view;
    let mut tree = Tree::new(&element);

    // 宽度硬约束（`min = max = layout_width`，compression.width = false）；
    // 高度压缩（`Length::Shrink` ⇒ `Fill` 退化为内在尺寸）。
    let limits = Limits::new(
        Size::new(layout_width, 0.0),
        Size::new(layout_width, max_height),
    )
    .height(Length::Shrink);

    Some(element.as_widget_mut().layout(&mut tree, renderer, &limits))
}

impl Host {
    /// 对话框窗口的推荐**逻辑尺寸**
    ///
    /// 决策规则见 [`decide_dialog_window_size`]——本方法只是把 `Host` 的视图与
    /// 渲染器喂给它，保证**生产路径与验证工具同源**（不出现两套口径）。
    pub fn measure_dialog_window_size(&self, work_area: Size) -> Size {
        decide_dialog_window_size(
            &self.root,
            &self.render_ctx.renderer,
            self.root.state.dialog_type,
            work_area,
        )
    }
}

/// 对话框窗口尺寸决策（REF-001：宽自动计算、高测量驱动）
///
/// # 规则
///
/// ```text
/// 宽 = clamp(内容不被压窄的最小宽度, width_floor, width_cap ∩ 工作区)
/// 高 = clamp(测量高度, 声明下限, 声明上限 ∩ 工作区)     // 钉死高度时跳过测量
/// ```
///
/// - 宽度区间退化（`boxed`/`pinned`/`floored`）时不跑搜索，直接用上限；
/// - 高度上限在**测量与夹取**两处共用同一个值
///   （[`dialog_preferred_size::measurement_cap`]），否则内容会在夹取那一步被裁掉；
/// - 钉死高度（`min_height == max_height`）完全不依赖测量。
///
/// 返回 `Size::ZERO` 表示**测量不可用**（`DialogType::None`、宽度非法、或内容高度
/// 非正/非有限），调用方必须回退种子尺寸并**照常显示窗口**。
///
/// 抽成自由函数是为了让**测试直接跑生产口径**（不需要构造 `Host`）：
/// [`dialog_sizes`] 的尺寸验证与 [`edge`] 的边缘可见性诊断都调用它。
pub fn decide_dialog_window_size(
    root: &crate::Root,
    renderer: &Renderer,
    dialog_type: crate::state::root_state::DialogType,
    work_area: Size,
) -> Size {
    let hint = dialog_preferred_size::dialog_size_hint(dialog_type);

    let width_cap = dialog_preferred_size::width_cap(hint, work_area);
    if !width_cap.is_finite() || width_cap <= 0.0 {
        return Size::ZERO;
    }

    let height_cap = dialog_preferred_size::measurement_cap(hint, work_area);
    let width_floor = dialog_preferred_size::width_floor(hint, work_area);

    // 宽度自动计算：区间内求「内容不被压窄的最小宽度」；
    // 区间退化为一点（`boxed`/`pinned`/`floored`）时直接取值，省掉基准 + 9 次布局。
    let width = if hint.needs_width_search(work_area.width) {
        match measure_min_fitting_width(
            || root.view(),
            renderer,
            height_cap,
            width_floor,
            width_cap,
        ) {
            Some(fitted) => fitted.clamp(width_floor, width_cap),
            None => width_floor,
        }
    } else {
        width_cap
    };

    let measured_height = measure_content_height(root.view(), renderer, width, height_cap);

    if !usable(Size::new(width, measured_height)) {
        tracing::warn!(
            "对话框内容测量不可用: {dialog_type:?} 宽度={width:.1} 上限={height_cap:.1} 实测高度={measured_height:.1}，调用方应回退种子尺寸"
        );
        return Size::ZERO;
    }

    dialog_preferred_size::recommend_window_size(hint, work_area, width, measured_height)
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod dialog_sizes;
#[cfg(test)]
mod edge;
