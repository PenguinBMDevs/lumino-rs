//! egui 整窗外壳（唯一生产 UI 层）。
//!
//! 在真实窗口 surface / 真实 `Device` 上运行 `lumino-egui-bridge`；外壳 band、
//! 对话框、面板与编辑器骨架全部由 egui 绘制，编辑器画布底层采样 gfx 离屏纹理。
//!
//! # 输入
//!
//! 指针 / 滚轮 / 修饰键 / 焦点同步给 egui；浮层（菜单 / 弹层 / 模态）的命中
//! 由 `ctx.layer_id_at` 判定，画布输入在浮层覆盖时让位。

use egui::Rect;
use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::icons::IconCache;
use wgpu;

use crate::host::Host;

mod actions;
mod canvas;
mod clock;
mod draw;
mod host;
mod ime;
pub(crate) mod input;

pub use actions::OverlayActions;
use ime::apply_ime_to_window;
#[cfg(test)]
use ime::ime_cursor_area;
pub(crate) use ime::input_scale;
pub use input::OverlayInput;

/// 一帧叠加层绘制结果（渲染之外的宿主副作用）。
pub(crate) struct OverlayOutput {
    /// egui 请求的系统光标。
    pub cursor: egui::CursorIcon,
    /// 文本编辑中的 IME 锚点（`Some` = 应开启系统输入法）。
    pub ime: Option<egui::output::IMEOutput>,
}

/// 一帧叠加层绘制输入。
pub(super) struct OverlayFrame<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size_px: [u32; 2],
    pixels_per_point: f32,
    app_frame: u32,
    model: &'a ChromeModel,
    /// 本帧色板（跟随窗口主题）。
    palette: lumino_ui_egui::theme::Palette,
    /// 渲染器预览 / 全屏瀑布流的离屏纹理（二者互斥；就绪时才注册）。
    render_preview: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 钢琴瀑布流离屏纹理（面板可见且已渲染时才注册）。
    waterfall: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 音频导出面板（整块接管右侧内容区）。
    audio_export: Option<lumino_ui_egui::chrome::AudioExportPanelModel>,
    /// 形状抽屉左侧面板。
    shape_drawer: Option<lumino_ui_egui::chrome::ShapeDrawerModel>,
    /// 形状抽屉右侧画布（egui painter 版）。
    shape_canvas: Option<lumino_ui_egui::shape_canvas::ShapeCanvasModel>,
    /// 视频导出面板（整块替换右侧内容区）。
    video_export: Option<lumino_ui_egui::chrome::VideoExportPanelModel>,
    /// 渲染器入口面板（轨道 / 16:9 预览 / 时间轴卡片 / 导出设置）。
    renderer_panel: Option<lumino_ui_egui::renderer_panel::RendererPanelModel>,
    /// 编辑器画布（卷帘 / 走带）离屏纹理；`Some` 时 egui 整窗采样它作底层。
    editor_texture: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 走带标尺模型；`Some` 且开关打开时由 egui 绘制。
    arrangement: Option<lumino_ui_egui::chrome::ArrangementModel>,
    /// 遮罩参数面板；`Some` 时由 egui 绘制并接管输入。
    /// 右下角 Toast 卡片（空 = 不显示）。
    toast: lumino_ui_egui::chrome::ToastModel,
    /// 首次使用帮助提示（None = 不显示）。
    help_tip: Option<lumino_ui_egui::chrome::HelpTipModel>,
    /// 「关于」页 logo 彩蛋飞行层（None = 不在飞行中）。
    egg: Option<lumino_ui_egui::chrome::EggModel>,
    /// 钢琴卷帘右键菜单（None = 未打开）。
    piano_roll_menu: Option<lumino_ui_egui::piano_roll_context_menu::PianoRollMenuModel>,
    /// 编辑器悬浮 √× 按钮（空 = 不显示）。
    editor_buttons: lumino_ui_egui::editor_action_buttons::EditorButtonsModel,
    /// 框选框（空 = 不显示）。
    marquee: lumino_ui_egui::marquee::MarqueeModel,
    /// 选中框 / 编辑框 / 8+1 锚点（空 = 不显示）。
    shape_edit: lumino_ui_egui::shape_edit::ShapeEditModel,
    /// 形状列表（只读；空 = 不显示）。
    shape_list: lumino_ui_egui::shape_list::ShapeListModel,
    /// 区域着色调色板（空 = 不显示）。
    area_palette: lumino_ui_egui::area_palette::AreaPaletteModel,
    /// 形状工具预览（空 = 不显示）。
    shape_preview: lumino_ui_egui::shape_preview::ShapePreviewModel,
    /// 曲线工具预览（空 = 不显示）。
    curve_preview: lumino_ui_egui::curve_preview::CurvePreviewModel,
    /// 协作远程光标（空 = 无人在线）。
    remote_cursors: lumino_ui_egui::remote_cursors::RemoteCursorModel,
    /// 编辑器叠加层预览（切割线 / 刀框 / 角度柔化）。
    editor_overlay: lumino_ui_egui::editor_overlay::EditorOverlayModel,
    /// 图片转 MIDI 区域框。
    i2m_region: lumino_ui_egui::image_to_midi::I2mRegionModel,
    /// 曲线弯折按钮（空 = 不显示）。
    bend_buttons: lumino_ui_egui::bend_buttons::BendButtonsModel,
    /// 文字工具覆盖层（None = 不显示）。
    text_tool: Option<lumino_ui_egui::text_tool::TextToolModel>,
    /// 力度 / CC / Tempo 面板骨架（None = 不可见）。
    velocity_panel: Option<lumino_ui_egui::velocity_panel::VelocityPanelModel>,
    mask_panel: Option<lumino_ui_egui::mask_panel::MaskPanelModel>,
    /// 纹理预览（渲染器 16:9 卡片 / 全屏瀑布流）；`Some` 时由 egui 铺图。
    video_preview: Option<lumino_ui_egui::chrome::TexturePreviewModel>,
    /// 视频剪辑时间轴（真实矩形 + 模型）；`Some` 且开关打开时由 egui 绘制。
    video_timeline: Option<(
        egui::Rect,
        lumino_ui_egui::video_clip_timeline::VideoTimelineModel,
    )>,
    /// 主区域无任何视图：显示「无面板」默认第一屏（`None` = 不显示）。
    ///
    /// 模型携带「是否有工程」两态：无工程时右栏带操作按钮，有工程时只显示
    /// 品牌块（见 `first_screen::FirstScreenModel`）。
    first_screen: Option<lumino_ui_egui::chrome::first_screen::FirstScreenModel>,
    /// 本帧要提交给 egui 的指针 / 滚轮 / 修饰键 / 焦点事件。
    events: Vec<egui::Event>,
    /// 窗口是否持有焦点。
    focused: bool,
    /// 清屏色：`Some` 表示 egui 独占整窗（生产渲染路径），
    /// `None` 表示叠加在 上一代 `present` 之上（回退路径）。
    clear: Option<wgpu::Color>,
}

/// 叠加层状态（首次启用时创建，跨帧保留纹理与管线）。
pub(crate) struct EguiOverlay {
    /// egui 上下文。
    ctx: egui::Context,
    /// 图标纹理缓存（真实 SVG，按物理像素缓存）。
    icons: IconCache,
    /// 上次写入 egui 的 `pixels_per_point`（避免逐帧重设字体栅格）。
    pixels_per_point: f32,
    /// wgpu27 绘制桥。
    painter: Painter,
    /// 帧时钟：egui 的点击 / 长按 / 双击判定都是**时间**语义，
    /// 每帧必须把真实秒数填进 `RawInput::time`（见 `clock` 模块注释）。
    clock: clock::FrameClock,
    /// 诊断开关：`LUMINO_EGUI_LEGACY_CLOCK=1` 时退回「不填 `time`」的旧行为。
    ///
    /// 只用于真机 A/B 复现（同一份二进制对比修复前后），正常启动必须为 `false`。
    /// 构造时读一次，避免每帧查询环境变量。
    legacy_clock: bool,
    /// 诊断开关：`LUMINO_CLOCK_DIAG=1` 时每帧比对 egui 内部时钟与真实时间。
    clock_diag: bool,
    /// 叠加层已绘制帧数。
    frames: u32,
    /// 本帧 egui 占用区（菜单 / 弹层 / 面板 / 模态，逻辑点）。
    ///
    /// 命中判定归 egui 自己所有：`Host` 不再保存第二份矩形清单，只查询
    /// [`EguiOverlay::pointer_blocked`]。
    blocked: Vec<Rect>,
}

/// 形状抽屉面板与画布之间的分割线宽度（与生产 上一代 视图一致）。
pub(crate) const SPLITTER_W: f32 = 6.0;

#[cfg(test)]
mod tests {
    use egui::{Area, Context, Id, Order, Rect, pos2, vec2};

    /// 输入归属依赖的 egui 事实：浮层 `Area` 在 `layer_id_at` 里返回非
    /// `Background` 图层，而 root Ui / 画布所在层是 `Background`。
    ///
    /// 这条假设一旦被 egui 升级打破，宿主就会重新把弹层点击漏给画布——
    /// 用测试钉死，不靠记忆。
    #[test]
    fn layer_id_at_separates_foreground_areas_from_background() {
        let ctx = Context::default();
        // `Area` 首帧是 sizing pass（`interactable` 被临时置 false），必须跑两帧
        // 才能进入命中栈——生产侧 `Area::show` 会主动 `request_repaint`。
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.label("background");
                Area::new(Id::unique("test-foreground"))
                    .order(Order::Foreground)
                    .fixed_pos(pos2(100.0, 100.0))
                    .show(ui.ctx(), |ui| {
                        ui.set_min_size(vec2(80.0, 40.0));
                        ui.label("panel");
                    });
            });
            // `TexturesDelta` 必须消费掉，否则 drop 时 epaint 会 panic。
            output.textures_delta.clear();
        }

        let inside = ctx.layer_id_at(pos2(140.0, 120.0)).expect("浮层应存在");
        assert_ne!(
            inside.order,
            Order::Background,
            "浮层命中必须是非 Background 图层"
        );

        let outside = ctx.layer_id_at(pos2(10.0, 10.0)).expect("root 层应存在");
        assert_eq!(
            outside.order,
            Order::Background,
            "画布/root 层命中必须是 Background"
        );
    }

    /// IME 候选窗锚点必须按 DPI 换算到物理像素，且尺寸不小于 1px。
    #[test]
    fn ime_cursor_area_scales_to_physical_pixels() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(2.0, 18.0));
        let (position, size) = super::ime_cursor_area(rect, 1.25);
        assert_eq!((position.x, position.y), (13, 25));
        assert_eq!((size.width, size.height), (3, 23));

        let (position, size) = super::ime_cursor_area(rect, 0.0);
        assert_eq!((position.x, position.y), (10, 20), "非法 ppp 回退 1.0");
        assert_eq!((size.width, size.height), (2, 18));
    }

    /// egui 文本编辑开始 / 结束时，宿主必须开关系统输入法并锚定候选窗。
    #[test]
    fn ime_output_toggles_window_state() {
        use crate::host::HostWindow;
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };

        #[derive(Default)]
        struct Mock {
            allowed: AtomicBool,
            area: Mutex<Option<(i32, i32, u32, u32)>>,
        }

        impl HostWindow for Mock {
            fn request_redraw(&self) {}
            fn set_cursor(&self, _icon: winit::window::CursorIcon) {}
            fn set_cursor_visible(&self, _visible: bool) {}
            fn scale_factor(&self) -> f64 {
                1.0
            }
            fn is_maximized(&self) -> bool {
                false
            }
            fn set_ime_allowed(&self, allowed: bool) {
                self.allowed.store(allowed, Ordering::SeqCst);
            }
            fn set_ime_cursor_area(
                &self,
                position: winit::dpi::PhysicalPosition<i32>,
                size: winit::dpi::PhysicalSize<u32>,
            ) {
                *self.area.lock().expect("IME 区域锁不得中毒") =
                    Some((position.x, position.y, size.width, size.height));
            }
        }

        let mock = Mock::default();
        let mut allowed = false;
        let ime = egui::output::IMEOutput {
            purpose: egui::IMEPurpose::Normal,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 20.0)),
            cursor_rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(2.0, 18.0)),
            should_interrupt_composition: false,
        };

        super::apply_ime_to_window(&mock, &mut allowed, Some(ime), 1.25);
        assert!(allowed, "文本编辑开始必须开启输入法");
        assert!(mock.allowed.load(Ordering::SeqCst));
        assert_eq!(
            *mock.area.lock().expect("IME 区域锁不得中毒"),
            Some((13, 25, 3, 23)),
            "候选窗必须按 DPI 锚到光标处"
        );

        super::apply_ime_to_window(&mock, &mut allowed, None, 1.25);
        assert!(!allowed, "文本编辑结束必须关闭输入法");
        assert!(!mock.allowed.load(Ordering::SeqCst));
    }
}
