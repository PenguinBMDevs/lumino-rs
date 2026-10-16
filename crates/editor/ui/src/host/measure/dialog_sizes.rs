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
use crate::view::dialog_preferred_size::{self, dialog_size_hint};

use super::measure_content_height;

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

        // 与生产路径同源：同一份「测量上限」既用于测量也用于夹取
        let cap = dialog_preferred_size::measurement_cap(hint, WORK_AREA);
        let measured_height = measure_content_height(root.view(), &renderer, hint.width, cap);
        let size = dialog_preferred_size::recommend_window_size(
            hint,
            WORK_AREA,
            Size::new(hint.width, measured_height),
        );

        assert_eq!(
            size.width, hint.width,
            "{dialog_type:?} 宽度必须取自声明（测量得到的宽度不可用）"
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
            "  {dialog_type:?}: 内容高 {measured_height:.1} → 窗口 {:.0}×{:.0}（上限 {cap:.0}）",
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
    for dialog_type in [DialogType::Settings, DialogType::CloudBrowser] {
        let root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, &ui_config);
        let hint = dialog_size_hint(dialog_type);
        let cap = dialog_preferred_size::measurement_cap(hint, WORK_AREA);

        let limits =
            iced_core::layout::Limits::new(Size::new(hint.width, 0.0), Size::new(hint.width, cap))
                .height(iced_core::Length::Shrink);

        let mut element = root.view();
        let mut tree = iced_core::widget::Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);

        println!("=== {dialog_type:?} root={:?}", node.size());
        dump(&node, 1);
    }
}

fn dump(node: &iced_core::layout::Node, depth: usize) {
    if depth > 14 {
        return;
    }
    let size = node.size();
    println!(
        "{:indent$}size=({:.1},{:.1})",
        "",
        size.width,
        size.height,
        indent = depth * 2
    );
    for child in node.children() {
        dump(child, depth + 1);
    }
}
