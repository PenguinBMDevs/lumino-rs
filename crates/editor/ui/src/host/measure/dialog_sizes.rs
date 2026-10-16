//! REF-001 阶段 F：**17 种对话框的自动化尺寸验证**
//!
//! 验证四件事（对应卡的验收标准）：
//! 1. 宽度取自声明，且声明为有效值；
//! 2. 尺寸不超工作区，且受各自声明上限约束；
//! 3. **无滚动区的对话框不得裁剪内容**（窗口高度 ≥ 内容高度）；
//! 4. **无声明上限的对话框，内容高度不得恰好顶到工作区上限**——那是
//!    「文本按 0 宽折行」等测量失真的特征值（本卡在阶段 F 正是靠它抓到了
//!    一次真实回归：两轴同时压缩时宽度塌成 0，高度被虚高到 1040）。
//!
//! 必须用**真实 wgpu headless 渲染器**：文本度量在 null 渲染器下恒为 0
//! （`iced_core/src/renderer/null.rs`），量不出真实高度。无可用 GPU 适配器时
//! 整体跳过——与 `root::draw_toolbar` 的护栏测试同一考虑（CI 软渲染环境）。

use iced_core::{Font, Pixels, Size};
use lumino_core::storage::config::UiConfig;

use crate::Root;
use crate::state::root_state::DialogType;
use crate::view::dialog_preferred_size::{self, dialog_size_hint, width_cap, width_floor};

use super::{
    measure_content_height, measure_is_squeezed, measure_min_fitting_width, measure_width_baseline,
};

/// 参考工作区：1920×1040（1080p 屏去掉任务栏后的典型可见区域）
const WORK_AREA: Size = Size::new(1920.0, 1040.0);

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

/// 构建 headless iced 渲染器（不需要窗口）
///
/// 无可用 GPU 适配器时返回 `None`，调用方**跳过**而不是失败
/// （与 `root::draw_toolbar::tests::headless_renderer_with_font` 同一实现口径）。
fn headless_renderer() -> Option<crate::Renderer> {
    use iced_wgpu::graphics::Shell;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let adapter = rt
        .block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .ok()?;
    let (device, queue) = rt
        .block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lumino_ref001_dialog_size_device"),
            required_features: adapter.features() & wgpu::Features::default(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .ok()?;
    let engine = iced_wgpu::Engine::new(
        &adapter,
        device,
        queue,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        None,
        Shell::headless(),
    );

    Some(crate::Renderer::new(
        engine,
        Font::DEFAULT,
        Pixels::from(16),
    ))
}

/// 17 种对话框逐类验证尺寸规则，并打印实测表（供实施记录引用）。
#[test]
fn test_all_dialog_types_measure_within_bounds() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器（对话框尺寸验证需要真实文本度量）");
        return;
    };

    let ui_config = UiConfig::default();
    let mut rows = Vec::new();

    for dialog_type in ALL_DIALOGS {
        let root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, &ui_config);
        let hint = dialog_size_hint(dialog_type);

        // 与生产路径同源：同一份上限既用于测量也用于夹取
        let cap_height = dialog_preferred_size::measurement_cap(hint, WORK_AREA);
        let cap_width = width_cap(hint, WORK_AREA);
        let floor_width = width_floor(hint, WORK_AREA);

        // 超宽参考基准：区分「内容驱动」与「流体」节点
        let baseline = measure_width_baseline(|| root.view(), &renderer, cap_width, cap_height)
            .expect("超宽参考布局必须可测");

        // 宽度自动计算（区间退化为一点时直接取上限，不跑搜索）
        let width = if cap_width <= floor_width + 1.0 {
            cap_width
        } else {
            measure_min_fitting_width(
                || root.view(),
                &renderer,
                cap_height,
                floor_width,
                cap_width,
            )
            .unwrap_or(floor_width)
            .clamp(floor_width, cap_width)
        };

        let measured_height = measure_content_height(root.view(), &renderer, width, cap_height);
        let size =
            dialog_preferred_size::recommend_window_size(hint, WORK_AREA, width, measured_height);

        assert!(
            width >= floor_width - 0.5 && width <= cap_width + 0.5,
            "{dialog_type:?} 自动宽度 {width} 越出声明区间 [{floor_width}, {cap_width}]"
        );
        assert_eq!(
            size.width, width,
            "{dialog_type:?} 窗口宽度必须等于自动计算结果"
        );
        assert!(
            measured_height > 0.0 && measured_height.is_finite(),
            "{dialog_type:?} 内容高度测量不可用: {measured_height}（会退化成 0 尺寸窗口）"
        );
        assert!(
            size.width > 0.0 && size.height > 0.0,
            "{dialog_type:?} 尺寸不可用: {size:?}"
        );
        assert!(
            size.height >= dialog_preferred_size::MIN_DIALOG_HEIGHT,
            "{dialog_type:?} 高度 {} 低于可点击下限",
            size.height
        );
        assert!(
            size.height <= WORK_AREA.height,
            "{dialog_type:?} 高度 {} 超出工作区 {}",
            size.height,
            WORK_AREA.height
        );
        // 宽度自动计算的核心不变量：选中的宽度不得压窄内容驱动的节点
        assert_eq!(
            measure_is_squeezed(|| root.view(), &renderer, size.width, cap_height, &baseline),
            Some(false),
            "{dialog_type:?} 自动宽度 {} 仍压窄了内容（声明上限需放宽）",
            size.width
        );

        // 且该宽度必须是**最小**的：再窄一点就应当被压窄（下限处已无压缩空间时跳过）
        if size.width > floor_width + 8.0 {
            assert_eq!(
                measure_is_squeezed(
                    || root.view(),
                    &renderer,
                    size.width - 8.0,
                    cap_height,
                    &baseline,
                ),
                Some(true),
                "{dialog_type:?} 的自动宽度 {} 不是最小可用宽度（还能更窄）",
                size.width
            );
        }

        match (hint.min_height, hint.max_height) {
            // 钉死高度（滚动区测量不可信）：无论测到多少，窗口高度恒为声明值
            (Some(min_height), Some(max_height)) if min_height == max_height => assert_eq!(
                size.height, max_height,
                "{dialog_type:?} 已钉死高度 {max_height}，实测 {}",
                size.height
            ),
            // 含无界滚动区：必须被声明上限夹住
            (_, Some(max_height)) => assert!(
                size.height <= max_height,
                "{dialog_type:?} 高度 {} 超出声明上限 {max_height}",
                size.height
            ),
            // 无滚动区：既不得裁剪内容，也不得虚高到顶住工作区
            (_, None) => {
                assert!(
                    size.height + 0.5 >= measured_height,
                    "{dialog_type:?} 裁剪了内容：窗口高 {} < 内容高 {measured_height}",
                    size.height
                );
                assert!(
                    measured_height < WORK_AREA.height,
                    "{dialog_type:?} 内容高 {measured_height} 恰好顶到工作区上限——\
                     这是测量失真的特征（例如文本被按 0 宽折行）；\
                     若内容确实这么高，应改用 pinned 钉死高度并套滚动区"
                );
            }
        }

        rows.push(format!(
            "  {dialog_type:?}: 自动宽 {width:.0}（区间 [{floor_width:.0}, {cap_width:.0}]）\
             + 内容高 {measured_height:.1} → 窗口 {:.0}×{:.0}",
            size.width, size.height
        ));
    }

    println!("REF-001 对话框尺寸实测（工作区 {WORK_AREA:?}）:");
    for row in &rows {
        println!("{row}");
    }
}

/// 人工排查工具：dump 指定对话框的布局树（默认忽略，CI 不跑）
///
/// 用法：`cargo test -p lumino-ui --lib debug_dump_dialog_layout_tree -- --ignored --nocapture`
#[test]
#[ignore = "人工排查工具，需要时用 --ignored 显式运行"]
fn debug_dump_dialog_layout_tree() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let ui_config = UiConfig::default();
    for (dialog_type, layout_width) in [
        (DialogType::BatchEdit, 247.0_f32),
        (DialogType::BatchEdit, 480.0),
        (DialogType::SaveConfirm, 247.0),
    ] {
        let root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, &ui_config);
        let hint = dialog_size_hint(dialog_type);
        let cap = dialog_preferred_size::measurement_cap(hint, WORK_AREA);

        let limits = iced_core::layout::Limits::new(
            Size::new(layout_width, 0.0),
            Size::new(layout_width, cap),
        )
        .height(iced_core::Length::Shrink);

        let mut element = root.view();
        let mut tree = iced_core::widget::Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);

        let squeezed = measure_width_baseline(|| root.view(), &renderer, 1920.0, cap)
            .and_then(|baseline| {
                measure_is_squeezed(|| root.view(), &renderer, layout_width, cap, &baseline)
            })
            .unwrap_or(false);

        println!(
            "=== {dialog_type:?} @w={layout_width} root={:?} 被压窄={squeezed}",
            node.size()
        );
        dump(&node, 1);
    }
}

fn dump(node: &iced_core::layout::Node, depth: usize) {
    if depth > 9 {
        return;
    }
    let bounds = node.bounds();
    println!(
        "{:indent$}size=({:.1},{:.1}) at=({:.1},{:.1}) 右={:.1}",
        "",
        bounds.width,
        bounds.height,
        bounds.x,
        bounds.y,
        bounds.x + bounds.width,
        indent = depth * 2
    );
    for child in node.children() {
        dump(child, depth + 1);
    }
}
