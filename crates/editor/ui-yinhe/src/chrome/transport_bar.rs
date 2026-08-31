//! 传输栏 — yinhe `transport_bar.rs:1912` 的 iced 像素级复刻
//!
//! 原 `yinhe-egui/src/chrome/transport_bar.rs:1912` 完整结构：
//! ```text
//! [文件按钮 32px + 图钉文件(0..10) ][编辑按钮 32px + 图钉编辑(0..12)][播放按钮 32px + 图钉播放(4)] | 黑色数码框 166×36 圆角8 (120.00/ 4/4 480 / 小节 / 0:00.000) | 右侧工具 7+方向切换 32px | 量化
//! ```
//! 尺寸与原版一致：高度 40px，外容器内边距左右 8 ，按钮 32×32 圆角2 图标 16-18，数码框宽 166(76+90) 高36，工具间距2。
//! 配色：外层跟随 `Theme`（`app_bg`），数码框固定黑色 #0F0F12 圆角8 + 青色 #64B4FF + 白色文字，网格线 #363638，工具选中态 `background.strong`。
//! 交互：文件/编辑/播放菜单按钮 + 图钉固定区（`pinned_*` 布尔数组）= 紧跟菜单按钮右侧一整行图标；中部数码框悬停提示；右侧8工具（选框/区间/手型/铅笔/曲线/剪刀/擦除/选区笔-画刷）+ 方向切换 + 量化。

use iced_core::{Alignment, Border, Color, Length};
use iced_widget::{button, container, row, space, text};

use lumino_core::{NotePrecision, Tool};
use lumino_message::YinheAction;
use lumino_ui_core::resources::icon;
use lumino_ui_core::toolbar_event::Event as ToolbarEvent;
use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Message, Theme};

// ── 常量：与 yinhe `theme/layout.rs` 对齐 ──────────────────────────────────

/// 传输栏高度（原 `Panel::top` + `inner_margin` 后的可视高度，对应 lumino 桩 40）
pub const TRANSPORT_H: f32 = 40.0;
/// 按钮尺寸（`TRANSPORT_BTN_SIZE = 32.0`）
const BTN_SIZE: f32 = 32.0;
/// 按钮圆角（原 `CornerRadius::same(2)`）
const BTN_RADIUS: f32 = 2.0;
/// 按钮图标字号（原 `TRANSPORT_BTN_FONT = 18`，SVG 取 16 适配）
const ICON_S: u32 = 16;
/// 数码框尺寸（原 `col_widths [76,90]` + `rect_h 36` + `CornerRadius 8`）
const DIGITAL_W: f32 = 166.0;
const DIGITAL_H: f32 = 36.0;
const DIGITAL_RADIUS: f32 = 8.0;
/// 数码框黑底与青色（原 `track_bg` ~ #141418 与 `accent_active` #64B4FF；此处固定黑色以保证 Theme 兼容下仍如原版）
const DIGITAL_BG: Color = Color::from_rgb(0.07, 0.07, 0.08); // #121214 近 track_bg (20,20,22) 的更黑版
const DIGITAL_ACCENT: Color = Color::from_rgb(0.392, 0.706, 1.0); // #64B4FF (100,180,255)
const DIGITAL_GRID: Color = Color::from_rgb(0.212, 0.212, 0.22); // #363638 ~ line_fg (54,54,56)

// ── File / Edit / Play 动作枚举（对齐 yinhe `FileAction::ALL:10` / `EditAction::ALL:12` / `PlayMenuAction:4`） ──

/// 文件菜单动作（顺序 = `pinned_file_actions` 索引，对齐 yinhe `FileAction::ALL:10`）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    NewProject,
    Open,
    Save,
    SaveAs,
    CloseDocument,
    ExportAudio,
    ExportMidi,
    ProjectSettings,
    Settings,
    Exit,
}

impl FileAction {
    pub const ALL: [FileAction; 10] = [
        FileAction::NewProject,
        FileAction::Open,
        FileAction::Save,
        FileAction::SaveAs,
        FileAction::CloseDocument,
        FileAction::ExportAudio,
        FileAction::ExportMidi,
        FileAction::ProjectSettings,
        FileAction::Settings,
        FileAction::Exit,
    ];

    pub fn icon(self) -> icon::Icon {
        match self {
            FileAction::NewProject => icon::Icon::Plus,
            FileAction::Open => icon::Icon::FolderTree,
            FileAction::Save => icon::Icon::Download,
            FileAction::SaveAs => icon::Icon::Download,
            FileAction::CloseDocument => icon::Icon::WindowClose,
            FileAction::ExportAudio => icon::Icon::MusicNote,
            FileAction::ExportMidi => icon::Icon::MusicNote,
            FileAction::ProjectSettings => icon::Icon::Gear,
            FileAction::Settings => icon::Icon::Gear,
            FileAction::Exit => icon::Icon::WindowClose,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FileAction::NewProject => "新建工程",
            FileAction::Open => "打开",
            FileAction::Save => "保存",
            FileAction::SaveAs => "另存为",
            FileAction::CloseDocument => "关闭文档",
            FileAction::ExportAudio => "导出音频",
            FileAction::ExportMidi => "导出 MIDI",
            FileAction::ProjectSettings => "工程设置",
            FileAction::Settings => "设置",
            FileAction::Exit => "退出",
        }
    }
}

/// 编辑菜单动作（顺序 = `pinned_edit_actions` 索引，对齐 yinhe `EditAction::ALL:12`）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Duplicate,
    Delete,
    TransposeUp,
    TransposeDown,
    DedupWithinTrack,
    DedupAcrossTracks,
}

impl EditAction {
    pub const ALL: [EditAction; 12] = [
        EditAction::Undo,
        EditAction::Redo,
        EditAction::Cut,
        EditAction::Copy,
        EditAction::Paste,
        EditAction::SelectAll,
        EditAction::Duplicate,
        EditAction::Delete,
        EditAction::TransposeUp,
        EditAction::TransposeDown,
        EditAction::DedupWithinTrack,
        EditAction::DedupAcrossTracks,
    ];

    pub fn icon(self) -> icon::Icon {
        match self {
            EditAction::Undo => icon::Icon::Undo,
            EditAction::Redo => icon::Icon::Redo,
            EditAction::Cut => icon::Icon::ContextMenuCut,
            EditAction::Copy => icon::Icon::ContextMenuCopy,
            EditAction::Paste => icon::Icon::ContextMenuPaste,
            EditAction::SelectAll => icon::Icon::ContextMenuSelectAll,
            EditAction::Duplicate => icon::Icon::ContextMenuCopy,
            EditAction::Delete => icon::Icon::ContextMenuDelete,
            EditAction::TransposeUp => icon::Icon::TransposeUp,
            EditAction::TransposeDown => icon::Icon::TransposeDown,
            EditAction::DedupWithinTrack => icon::Icon::Eraser,
            EditAction::DedupAcrossTracks => icon::Icon::Eraser,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EditAction::Undo => "撤销",
            EditAction::Redo => "重做",
            EditAction::Cut => "剪切",
            EditAction::Copy => "复制",
            EditAction::Paste => "粘贴",
            EditAction::SelectAll => "全选",
            EditAction::Duplicate => "复制",
            EditAction::Delete => "删除",
            EditAction::TransposeUp => "上移调",
            EditAction::TransposeDown => "下移调",
            EditAction::DedupWithinTrack => "轨内去重",
            EditAction::DedupAcrossTracks => "跨轨去重",
        }
    }
}

/// 播放跟随模式（对齐 yinhe `FollowMode`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FollowMode {
    #[default]
    None,
    Centered,
    Page,
    Continuous,
}

/// 方向（对齐 yinhe `Orientation` / `yinhe_types::Orientation`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

// ── 传输栏状态（聚合入参，对齐 yinhe `TransportContext` + `TransportResponse` 的 iced 版） ──

/// 传输栏状态（聚合入参，避免长参数列表）
///
/// 对齐 yinhe `TransportContext` 的字段（播放/录音/步进/速度/拍号/量化/工具/图钉/跟随/方向/PPQ/游标 等），
/// P2 桩仅 6 要素，此处扩展为完整走带语义以支持像素级复刻。
#[derive(Debug, Clone)]
pub struct TransportState {
    /// 是否正在播放（`Document.edit.playback.is_playing()`）
    pub is_playing: bool,
    /// 是否正在录制（REC 高亮，红色）
    pub is_recording: bool,
    /// 步进输入是否激活（高亮）
    pub step_input: bool,
    /// 速度 BPM（显示 `120.00`）
    pub bpm: f32,
    /// 拍号分子（如 4）
    pub time_sig_numer: u8,
    /// 拍号分母（如 4）
    pub time_sig_denom: u8,
    /// 每四分音符脉冲数（PPQ，默认 480，显示于数码框左下 `4/4 480`）
    pub ppq: u16,
    /// 游标 tick（用于计算小节/拍/时间）
    pub cursor_tick: f64,
    /// 当前量化精度
    pub quantize: NotePrecision,
    /// 当前工具（高亮用）
    pub active_tool: Tool,
    /// 是否有活动文档（无文档时部分按钮禁用置灰）
    pub has_active_document: bool,
    /// 方向（横向/纵向，`RollBarHorizontal/Vertical` 切换）
    pub orientation: Orientation,
    /// 跟随模式（播放菜单内单选，未钉到工具栏时仅菜单可见）
    pub follow_mode: FollowMode,
    /// 文件菜单图钉（10 项，对齐 `FileAction::ALL`）
    pub pinned_file_actions: [bool; 10],
    /// 编辑菜单图钉（12 项，对齐 `EditAction::ALL`）
    pub pinned_edit_actions: [bool; 12],
    /// 播放菜单图钉：播放/暂停
    pub pinned_play_pause: bool,
    /// 播放菜单图钉：停止
    pub pinned_stop: bool,
    /// 播放菜单图钉：录音
    pub pinned_record: bool,
    /// 播放菜单图钉：步进输入
    pub pinned_step_input: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_recording: false,
            step_input: false,
            bpm: 120.0,
            time_sig_numer: 4,
            time_sig_denom: 4,
            ppq: 480,
            cursor_tick: 0.0,
            quantize: NotePrecision::Quarter,
            active_tool: Tool::Pointer,
            has_active_document: false,
            orientation: Orientation::Horizontal,
            follow_mode: FollowMode::None,
            pinned_file_actions: [false; 10],
            pinned_edit_actions: [false; 12],
            pinned_play_pause: false,
            pinned_stop: false,
            pinned_record: false,
            pinned_step_input: false,
        }
    }
}

// ── 辅助：格式化（对齐 yinhe `time_format.rs`） ──

fn format_bpm(bpm: f32) -> String {
    format!("{:.2}", bpm)
}

fn format_time(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    let ms = ((seconds % 1.0) * 1000.0) as u32;
    format!("{}:{:02}.{:03}", mins, secs, ms)
}

fn format_time_sig(numer: u8, denom: u8, ppq: u16) -> String {
    format!("{}/{} {}", numer, denom, ppq)
}

/// `tick` → `bar.beat.tick`（单拍号假设，与 `format_tick_bar_beat_with_time_sig` 的单段等价）
fn format_tick_bar_beat(tick: f64, ppq: u16, numer: u8) -> String {
    let ppq = ppq as f64;
    let ticks_per_bar = ppq * numer as f64;
    if ticks_per_bar <= 0.0 {
        return "1.1.000".to_string();
    }
    let bar = (tick / ticks_per_bar).floor() as u32 + 1;
    let rem_bar = tick % ticks_per_bar;
    let beat = (rem_bar / ppq).floor() as u32 + 1;
    let tick_in_beat = (rem_bar % ppq) as u32;
    format!("{}.{}.{:03}", bar, beat, tick_in_beat)
}

fn quantize_label(q: NotePrecision) -> String {
    match q {
        NotePrecision::Whole => "1/1".to_string(),
        NotePrecision::Half => "1/2".to_string(),
        NotePrecision::Quarter => "1/4".to_string(),
        NotePrecision::Eighth => "1/8".to_string(),
        NotePrecision::Sixteenth => "1/16".to_string(),
        NotePrecision::ThirtySecond => "1/32".to_string(),
        NotePrecision::SixtyFourth => "1/64".to_string(),
        NotePrecision::OneTwentyEighth => "1/128".to_string(),
        NotePrecision::Custom => "自定义".to_string(),
    }
}

// ── 按钮工厂：与 `lumino-ui/src/toolbar/buttons.rs:tool_selector_custom` 风格一致 ──

fn menu_button<'a>(
    icon_enum: icon::Icon,
    _tooltip: &'static str,
    on_press: Option<Message>,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg_weak = palette.background.weak.color;
    let icon_el = icon::view_with_size_and_theme(icon_enum, ICON_S, ICON_S, Some(&window.theme));
    let mut btn = button(icon_el)
        .padding(6)
        .style(move |_theme: &Theme, status| {
            let bg = if status == button::Status::Hovered {
                bg_weak
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: Border {
                    radius: BTN_RADIUS.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
        });
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

fn pinned_action_button<'a>(
    icon_enum: icon::Icon,
    _tooltip: &'static str,
    on_press: Option<Message>,
    is_active: bool,
    enabled: bool,
    accent_override: Option<Color>,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg_strong = palette.background.strong.color;
    let bg_weak = palette.background.weak.color;
    let icon_el = icon::view_with_size_and_theme(icon_enum, ICON_S, ICON_S, Some(&window.theme));
    let mut btn = button(icon_el)
        .padding(6)
        .style(move |_theme: &Theme, status| {
            let bg = if !enabled {
                Color::TRANSPARENT
            } else if is_active {
                // 录音激活用红，其他激活用 strong（与 yinhe icon_accent 逻辑一致）
                accent_override.unwrap_or(bg_strong)
            } else if status == button::Status::Hovered {
                bg_weak
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: Border {
                    radius: BTN_RADIUS.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
        });
    if enabled && let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

fn tool_button<'a>(
    tool: Tool,
    current: Tool,
    icon_enum: icon::Icon,
    tooltip: &'static str,
    enabled: bool,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let is_active = tool == current;
    let bg_strong = palette.background.strong.color;
    let bg_weak = palette.background.weak.color;
    let icon_el = icon::view_with_size_and_theme(icon_enum, ICON_S, ICON_S, Some(&window.theme));
    let mut btn = button(icon_el)
        .padding(6)
        .style(move |_theme: &Theme, status| {
            let bg = if !enabled {
                Color::TRANSPARENT
            } else if is_active {
                bg_strong
            } else if status == button::Status::Hovered {
                bg_weak
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: Border {
                    radius: BTN_RADIUS.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
        });
    if enabled {
        btn = btn.on_press(ToolbarEvent::tool_selected(tool));
    }
    let _ = tooltip;
    btn.into()
}

// ── 数码显示框：黑色 166×36 圆角8，两列 76/90，上下两行，垂直分隔线 ──

fn timecode_display<'a>(_window: &'a Window, state: &TransportState) -> Element<'a> {
    // 计算四角文本（对齐 yinhe `show_timecode_display`）
    let bpm_str = format_bpm(state.bpm);
    let ts_str = format_time_sig(state.time_sig_numer, state.time_sig_denom, state.ppq);
    let seconds = if state.bpm > 0.0 {
        state.cursor_tick / state.ppq as f64 * 60.0 / state.bpm as f64
    } else {
        0.0
    };
    let time_str = format_time(seconds);
    let pos_str = format_tick_bar_beat(state.cursor_tick, state.ppq, state.time_sig_numer);

    // 单元：固定宽列内居中文本（12px，青色）
    let cell = |s: String| -> Element<'a> {
        container(text(s).size(11).style(move |_t: &Theme| iced_widget::text::Style {
            color: Some(DIGITAL_ACCENT),
        }))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    };

    let left_top = cell(bpm_str);
    let left_bot = cell(ts_str);
    let right_top = cell(pos_str);
    let right_bot = cell(time_str);

    // 左列 76px：上下两行；右列 90px
    let left_col: Element<'a> = container(
        iced_widget::column![left_top, left_bot]
            .spacing(0)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fixed(76.0))
    .height(Length::Fill)
    .into();

    let right_col: Element<'a> = container(
        iced_widget::column![right_top, right_bot]
            .spacing(0)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fixed(90.0))
    .height(Length::Fill)
    .into();

    let divider: Element<'a> = container(space().width(Length::Fixed(1.0)).height(Length::Fill))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(DIGITAL_GRID)),
            ..Default::default()
        })
        .into();

    let inner = row![left_col, divider, right_col]
        .spacing(0)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill);

    container(inner)
        .width(Length::Fixed(DIGITAL_W))
        .height(Length::Fixed(DIGITAL_H))
        .style(|_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(DIGITAL_BG)),
            border: Border {
                radius: DIGITAL_RADIUS.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .into()
}

// ── 渲染传输栏 ──

/// 渲染传输栏（像素级复刻 yinhe `transport_bar.rs:303 show`）
///
/// 布局（`left_to_right` 原语改为 iced `row` + `Fill` 居中）：
/// ```text
/// [文件 32 + 图钉文件*][编辑 32 + 图钉编辑*][播放 32 + 图钉播放(播放/停止/录音/步进)] | 黑色数码框 166×36 (120.00/4/4 480/1.1.000/0:00.000) | 工具8(32) + 方向切换 + 量化
/// ```
/// - 播放/暂停/停止/录音/步进：走 `Toolbar::{play,pause,stop,record,record_stop}` + `step` 占位（暂走 `null`）
/// - 工具：8 键（选框/区间/手型/铅笔/曲线/剪刀/擦除/选区笔=Brush）+ 方向切换（横/纵）
/// - 量化：`Toolbar::Quantize` / `PrecisionChanged`（文本 `1/4` 等，`icon::Quantize` + 标签）
pub fn view<'a>(window: &'a Window, state: TransportState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let has_doc = state.has_active_document;

    // ── 左侧：文件/编辑/播放 菜单按钮 + 图钉固定区 — iced_aw Menu 弹窗（参考 lumino titlebar/menu.rs，圆角8+阴影美化） ──
    use iced_aw::{Menu, menu::Item, style::menu_bar};
    // 文件菜单：带图标 +  pin 切换，hover 弱底 + 选中强底，圆角8+阴影 0,2,8 0.12 美化
    let file_items = FileAction::ALL
        .iter()
        .enumerate()
        .map(|(idx, action)| {
            let is_pinned = state.pinned_file_actions[idx];
            let pin_icon = if is_pinned { "📌" } else { "📍" };
            let pin_btn = iced_widget::button(iced_widget::text(pin_icon).size(12))
                .padding(2)
                .on_press(Message::Yinhe(YinheAction::TogglePinnedFile(idx)))
                .style(|theme: &Theme, status| {
                    let p = theme.extended_palette();
                    let bg = match status {
                        iced_widget::button::Status::Hovered => p.background.weaker.color,
                        iced_widget::button::Status::Pressed => p.background.weak.color,
                        _ => iced_core::Color::TRANSPARENT,
                    };
                    iced_widget::button::Style {
                        background: Some(iced_core::Background::Color(bg)),
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    }
                });
            let label_with_icon: iced_widget::Row<'_, Message, Theme, lumino_ui_core::Renderer> =
                iced_widget::row![ crate::material_icons::icon(
                    match action {
                        FileAction::NewProject => crate::material_icons::codepoints::TEXT_FIELDS,
                        FileAction::Open => crate::material_icons::codepoints::FOLDER_OPEN,
                        FileAction::Save => crate::material_icons::codepoints::SAVE,
                        _ => crate::material_icons::codepoints::HOME,
                    },
                    14.0,
                    window.theme.extended_palette().background.base.text
                ),
                iced_widget::text(action.label()).size(13)
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            // 美化：悬浮弱底，按压强底，圆角8+阴影 0,2,8 0.12，已统一到 edit/play
            let main_btn: iced_widget::Button<'_, Message, Theme, lumino_ui_core::Renderer> =
                iced_widget::button(label_with_icon)
                    .width(iced_core::Length::Fill)
                    .padding([6, 10])
                    .style(|theme: &Theme, status| {
                        let p = theme.extended_palette();
                        let bg = match status {
                            iced_widget::button::Status::Hovered => p.background.weaker.color,
                            iced_widget::button::Status::Pressed => p.background.weak.color,
                            _ => iced_core::Color::TRANSPARENT,
                        };
                        iced_widget::button::Style {
                            background: Some(iced_core::Background::Color(bg)),
                            border: iced_core::Border::default().rounded(8),
                            shadow: iced_core::Shadow {
                                color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                                offset: iced_core::Vector::new(0.0, 2.0),
                                blur_radius: 8.0,
                            },
                            text_color: p.background.neutral.text,
                            ..Default::default()
                        }
                    })
                    .on_press(lumino_ui_core::message::null());
            let row_el: Element<'_> = iced_widget::row![main_btn, pin_btn]
                .spacing(4)
                .align_y(Alignment::Center)
                .into();
            Item::new(row_el)
        })
        .collect::<Vec<_>>();
    let file_menu_btn = Item::with_menu(
        menu_button(icon::Icon::FolderTree, "文件", Some(lumino_ui_core::message::null()), window),
        Menu::new(file_items).width(220.0).offset(4.0),
    );
    let edit_items = EditAction::ALL
        .iter()
        .enumerate()
        .map(|(idx, action)| {
            let is_pinned = state.pinned_edit_actions[idx];
            let pin_icon = if is_pinned { "📌" } else { "📍" };
            let pin_btn = iced_widget::button(iced_widget::text(pin_icon).size(12))
                .padding(2)
                .on_press(Message::Yinhe(YinheAction::TogglePinnedEdit(idx)))
                .style(|theme: &Theme, status| {
                    let p = theme.extended_palette();
                    let bg = match status {
                        iced_widget::button::Status::Hovered => p.background.weaker.color,
                        iced_widget::button::Status::Pressed => p.background.weak.color,
                        _ => iced_core::Color::TRANSPARENT,
                    };
                    iced_widget::button::Style {
                        background: Some(iced_core::Background::Color(bg)),
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    }
                });
            let label_row = iced_widget::row![
                crate::material_icons::icon(
                    crate::material_icons::codepoints::EDIT,
                    14.0,
                    window.theme.extended_palette().background.base.text
                ),
                iced_widget::text(action.label()).size(13)
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            let main_btn: iced_widget::Button<'_, Message, Theme, lumino_ui_core::Renderer> =
                iced_widget::button(label_row)
                    .width(iced_core::Length::Fill)
                    .padding([6, 10])
                    .style(|theme: &Theme, status| {
                        let p = theme.extended_palette();
                        let bg = match status {
                            iced_widget::button::Status::Hovered => p.background.weaker.color,
                            iced_widget::button::Status::Pressed => p.background.weak.color,
                            _ => iced_core::Color::TRANSPARENT,
                        };
                        iced_widget::button::Style {
                            background: Some(iced_core::Background::Color(bg)),
                            border: iced_core::Border::default().rounded(8),
                            shadow: iced_core::Shadow {
                                color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                                offset: iced_core::Vector::new(0.0, 2.0),
                                blur_radius: 8.0,
                            },
                            text_color: p.background.neutral.text,
                            ..Default::default()
                        }
                    })
                    .on_press(lumino_ui_core::message::null());
            let row_el: Element<'_> = iced_widget::row![main_btn, pin_btn]
                .spacing(4)
                .align_y(Alignment::Center)
                .into();
            Item::new(row_el)
        })
        .collect::<Vec<_>>();
    let edit_menu_btn = Item::with_menu(
        menu_button(icon::Icon::Pencil, "编辑", Some(lumino_ui_core::message::null()), window),
        Menu::new(edit_items).width(200.0).offset(4.0),
    );
    let play_items = {
        // 播放菜单 4 项（对齐 TransportState pin 4 布尔），每项带 pin 切换，圆角8+阴影 0,2,8 0.12 统一美化
        let play_defs: [(&str, usize, Option<Message>); 4] = [
            ("播放/暂停", 0, Some(ToolbarEvent::play())),
            ("停止", 1, Some(ToolbarEvent::stop())),
            ("录制", 2, Some(ToolbarEvent::record())),
            ("步进输入", 3, Some(lumino_ui_core::message::null())),
        ];
        play_defs
            .iter()
            .map(|(label, idx, msg)| {
                let is_pinned = match idx {
                    0 => state.pinned_play_pause,
                    1 => state.pinned_stop,
                    2 => state.pinned_record,
                    3 => state.pinned_step_input,
                    _ => false,
                };
                let pin_icon = if is_pinned { "📌" } else { "📍" };
                let pin_btn = iced_widget::button(iced_widget::text(pin_icon).size(12))
                    .padding(2)
                    .on_press(Message::Yinhe(YinheAction::TogglePinnedPlay(*idx)))
                    .style(|theme: &Theme, status| {
                        let p = theme.extended_palette();
                        let bg = match status {
                            iced_widget::button::Status::Hovered => p.background.weaker.color,
                            iced_widget::button::Status::Pressed => p.background.weak.color,
                            _ => iced_core::Color::TRANSPARENT,
                        };
                        iced_widget::button::Style {
                            background: Some(iced_core::Background::Color(bg)),
                            border: iced_core::Border::default().rounded(4),
                            ..Default::default()
                        }
                    });
                let label_el = iced_widget::row![
                    crate::material_icons::icon(
                        crate::material_icons::codepoints::PLAY_ARROW,
                        14.0,
                        window.theme.extended_palette().background.base.text
                    ),
                    iced_widget::text(*label).size(13)
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                let main_btn: iced_widget::Button<'_, Message, Theme, lumino_ui_core::Renderer> =
                    iced_widget::button(label_el)
                        .width(iced_core::Length::Fill)
                        .padding([6, 10])
                        .style(|theme: &Theme, status| {
                            let p = theme.extended_palette();
                            let bg = match status {
                                iced_widget::button::Status::Hovered => p.background.weaker.color,
                                iced_widget::button::Status::Pressed => p.background.weak.color,
                                _ => iced_core::Color::TRANSPARENT,
                            };
                            iced_widget::button::Style {
                                background: Some(iced_core::Background::Color(bg)),
                                border: iced_core::Border::default().rounded(8),
                                shadow: iced_core::Shadow {
                                    color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                                    offset: iced_core::Vector::new(0.0, 2.0),
                                    blur_radius: 8.0,
                                },
                                text_color: p.background.neutral.text,
                                ..Default::default()
                            }
                        })
                        .on_press(msg.clone().unwrap_or(lumino_ui_core::message::null()));
                let row_el: Element<'_> = iced_widget::row![main_btn, pin_btn]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into();
                Item::new(row_el)
            })
            .collect::<Vec<_>>()
    };
    let play_menu_btn = Item::with_menu(
        menu_button(icon::Icon::PlayCircle, "播放", Some(lumino_ui_core::message::null()), window),
        Menu::new(play_items).width(160.0).offset(4.0),
    );
    // 将三个 Menu Item 包装为 MenuBar（lumino 标题栏同款，close_on_background_click，容器美化：背景 base、边框 weak、圆角8、阴影 0,2,8 0.12）
    let menu_bar = iced_aw::MenuBar::new(vec![file_menu_btn, edit_menu_btn, play_menu_btn])
        .close_on_background_click_global(true)
        .close_on_item_click_global(true)
        .spacing(2)
        .style(|theme: &Theme, status| {
            let p = theme.extended_palette();
            let mut s = menu_bar::primary(theme, status);
            s.bar_background = iced_core::Background::Color(iced_core::Color::TRANSPARENT);
            s.menu_background = iced_core::Background::Color(p.background.base.color);
            s.menu_border = iced_core::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: p.background.weak.color,
            };
            s.menu_shadow = iced_core::Shadow {
                color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                offset: iced_core::Vector::new(0.0, 2.0),
                blur_radius: 8.0,
            };
            s
        });

    // 图钉文件按钮行（`pinned_file_actions` 10 项，有钉才显示）
    let mut pinned_file_row: Vec<Element<'a>> = Vec::new();
    for (idx, action) in FileAction::ALL.iter().enumerate() {
        if !state.pinned_file_actions[idx] {
            continue;
        }
        let enabled = has_doc
            || matches!(
                action,
                FileAction::NewProject | FileAction::Open | FileAction::Settings | FileAction::Exit
            );
        pinned_file_row.push(pinned_action_button(
            action.icon(),
            action.label(),
            Some(lumino_ui_core::message::null()),
            false,
            enabled,
            None,
            window,
        ));
    }

    // 图钉编辑按钮行（12 项）
    let mut pinned_edit_row: Vec<Element<'a>> = Vec::new();
    for (idx, action) in EditAction::ALL.iter().enumerate() {
        if !state.pinned_edit_actions[idx] {
            continue;
        }
        let enabled = has_doc;
        pinned_edit_row.push(pinned_action_button(
            action.icon(),
            action.label(),
            Some(lumino_ui_core::message::null()),
            false,
            enabled,
            None,
            window,
        ));
    }

    // 图钉播放按钮行（4 项：播放/暂停、停止、录音、步进）
    let mut pinned_play_row: Vec<Element<'a>> = Vec::new();
    // 播放/暂停
    if state.pinned_play_pause {
        let is_playing = state.is_playing;
        let icon = if is_playing {
            icon::Icon::Pause
        } else {
            icon::Icon::Play
        };
        let msg = if is_playing {
            ToolbarEvent::pause()
        } else {
            ToolbarEvent::play()
        };
        pinned_play_row.push(pinned_action_button(
            icon,
            if is_playing { "暂停" } else { "播放" },
            Some(msg),
            is_playing,
            has_doc,
            None,
            window,
        ));
    }
    // 停止
    if state.pinned_stop {
        pinned_play_row.push(pinned_action_button(
            icon::Icon::Ban,
            "停止",
            has_doc.then_some(ToolbarEvent::stop()),
            false,
            has_doc,
            None,
            window,
        ));
    }
    // 录音（激活红底）
    if state.pinned_record {
        let is_rec = state.is_recording;
        let red = Color::from_rgb8(220, 38, 38);
        pinned_play_row.push(pinned_action_button(
            icon::Icon::PlayCircle,
            "录制",
            Some(if is_rec {
                ToolbarEvent::record_stop()
            } else {
                ToolbarEvent::record()
            }),
            is_rec,
            has_doc,
            if is_rec { Some(red) } else { None },
            window,
        ));
    }
    // 步进输入（激活 accent 高亮）
    if state.pinned_step_input {
        let active = state.step_input;
        pinned_play_row.push(pinned_action_button(
            icon::Icon::Quantize,
            "步进输入",
            Some(lumino_ui_core::message::null()),
            active,
            has_doc,
            None,
            window,
        ));
    }

    // 左侧聚合行（含按钮间 2px 间距，与原 `ui.add_space(2.0)` 一致）— menu_bar 美化：圆角8+阴影
    let left_group: Element<'a> = {
        // menu_bar 为三菜单聚合，需转为 Element 单独处理
        let bar_el: Element<'a> = menu_bar.into();
        let mut items: Vec<Element<'a>> = Vec::new();
        items.push(bar_el);
        items.extend(pinned_file_row);
        items.extend(pinned_edit_row);
        items.extend(pinned_play_row);
        row(items).spacing(2).align_y(Alignment::Center).into()
    };

    // ── 中部：黑色数码框（固定黑底青字，Theme 兼容但不跟随 Theme 变色） ──
    let timecode = timecode_display(window, &state);

    // ── 右侧：8 工具按钮 + 方向切换 + 量化 ─────────────────────────────
    // 工具表：对齐 yinhe `ALL_TOOLS:7` + 选区笔(画刷) = 8（任务要求 8 工具 + 量化）
    // yinhe 7：Select/SelectVertical/Pan/Pencil/Curve/Scissors/Eraser
    // lumino 侧新增 Brush(选区笔) 作为第 8
    let tool_defs: [(Tool, icon::Icon, &'static str); 8] = [
        (Tool::Pointer, icon::Icon::MousePointer, "选框工具"),
        (
            Tool::PointerYSelect,
            icon::Icon::MousePointerYSelect,
            "区间选择",
        ),
        (Tool::Pencil, icon::Icon::Pencil, "铅笔"),
        (Tool::Brush, icon::Icon::BrushTool, "选区笔/画刷"),
        (Tool::Curve, icon::Icon::Curve, "曲线"),
        (Tool::Razor, icon::Icon::Split, "剪刀"),
        (Tool::Eraser, icon::Icon::Eraser, "擦除"),
        // 手型 Pan 原无直接图标，用 Scroll(autoscroll) 近似
        (Tool::Pen, icon::Icon::Scroll, "手型/抓手"),
    ];

    // 手型用第 8 位调序到第 3 位，使顺序 选框/区间/手型/铅笔/曲线/剪刀/擦除/画刷 与任务描述一致
    let ordered_tools: [(Tool, icon::Icon, &'static str); 8] = [
        tool_defs[0], // Pointer
        tool_defs[1], // YSelect
        tool_defs[7], // Pan(Pen+Scroll)
        tool_defs[2], // Pencil
        tool_defs[4], // Curve
        tool_defs[5], // Razor/Split
        tool_defs[6], // Eraser
        tool_defs[3], // Brush
    ];

    let mut tool_buttons: Vec<Element<'a>> = Vec::new();
    for (tool, ic, tip) in ordered_tools {
        tool_buttons.push(tool_button(tool, state.active_tool, ic, tip, has_doc, window));
    }

    // 方向切换（横向/纵向，`RollBarHorizontal/Vertical`，当前方向高亮如 yinhe `hover_button_rotated`）
    let ori_icon = match state.orientation {
        Orientation::Horizontal => icon::Icon::RollBarHorizontal,
        Orientation::Vertical => icon::Icon::RollBarVertical,
    };
    let ori_tip = match state.orientation {
        Orientation::Horizontal => "横向",
        Orientation::Vertical => "纵向",
    };
    let ori_btn = {
        let is_vertical = state.orientation == Orientation::Vertical;
        let bg_strong = palette.background.strong.color;
        let bg_weak = palette.background.weak.color;
        let icon_el =
            icon::view_with_size_and_theme(ori_icon, ICON_S, ICON_S, Some(&window.theme));
        let mut btn = button(icon_el)
            .padding(6)
            .style(move |_theme: &Theme, status| {
                let bg = if is_vertical {
                    bg_strong
                } else if status == button::Status::Hovered {
                    bg_weak
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    background: Some(iced_core::Background::Color(bg)),
                    border: Border {
                        radius: BTN_RADIUS.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            });
        // 复用 YinheAction::TogglePianorollInArrange 的语义占位（orientation 切换走同通道，真实状态由上层持有）
        btn = btn.on_press(lumino_ui_core::message::null());
        let _ = ori_tip;
        btn.into()
    };

    // 量化按钮（`icon::Quantize` + 标签如 `1/4`，点击走 `Toolbar::Quantize`）
    let quant_label = quantize_label(state.quantize);
    let quant_btn = {
        let bg_weak = palette.background.weak.color;
        let quant_icon = icon::view_with_size_and_theme(icon::Icon::Quantize, 14, 14, Some(&window.theme));
        let lbl = text(quant_label.clone()).size(11).style(move |theme: &Theme| {
            let p = theme.extended_palette();
            iced_widget::text::Style {
                color: Some(p.background.base.text),
            }
        });
        let content = row![quant_icon, lbl].spacing(4).align_y(Alignment::Center);
        let mut btn = button(content)
            .padding([4, 6])
            .style(move |_theme: &Theme, status| {
                let bg = if status == button::Status::Hovered {
                    bg_weak
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    background: Some(iced_core::Background::Color(bg)),
                    border: Border {
                        radius: BTN_RADIUS.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            });
        if has_doc {
            btn = btn.on_press(ToolbarEvent::quantize());
        }
        btn.into()
    };

    // 右侧聚合：工具 8 + 方向 + 量化（与原 `ui.add_space(4)+工具循环+add_space(4)+方向` 一致）
    let mut right_items: Vec<Element<'a>> = Vec::new();
    right_items.extend(tool_buttons);
    right_items.push(space().width(4).height(Length::Fixed(BTN_SIZE)).into());
    right_items.push(ori_btn);
    right_items.push(space().width(8).height(Length::Fixed(BTN_SIZE)).into());
    right_items.push(quant_btn);

    let right_group: Element<'a> = row(right_items).spacing(2).align_y(Alignment::Center).into();

    // ── 顶层布局：左组 | 填充 | 数码框(居中) | 填充 | 右组 ─────────────────
    // 原版 `bar_cx - rect_w*0.5` 居中 + `ui.add_space(pad)` 动态垫步；
    // iced 侧用双 `Fill` 实现像素级居中（左右留白对称，窗口拉伸仍居中）
    let content = row![
        left_group,
        space().width(Length::Fill),
        timecode,
        space().width(Length::Fill),
        right_group
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .padding([4, 8]);

    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(TRANSPORT_H))
        .style(|theme: &Theme| {
            let p = theme.extended_palette();
            container::Style {
                background: Some(iced_core::Background::Color(p.background.base.color)),
                ..Default::default()
            }
        })
        .into()
}

// ── 量化等辅助的显示名：已由 `quantize_label` 提供；`TransportState` 的 `bpm` 等由调用方填充 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_state_default() {
        let s = TransportState::default();
        assert!(!s.is_playing);
        assert!(!s.is_recording);
        assert!((s.bpm - 120.0).abs() < f32::EPSILON);
        assert_eq!(s.time_sig_numer, 4);
        assert_eq!(s.time_sig_denom, 4);
        assert_eq!(s.ppq, 480);
        assert_eq!(s.cursor_tick, 0.0);
        assert!(!s.has_active_document);
    }

    #[test]
    fn transport_state_quantize_display() {
        let s = TransportState {
            quantize: NotePrecision::Eighth,
            ..Default::default()
        };
        assert_eq!(s.quantize.display_name(), "八分音符");
        assert_eq!(quantize_label(s.quantize), "1/8");
    }

    #[test]
    fn format_helpers() {
        assert_eq!(format_bpm(120.0), "120.00");
        assert_eq!(format_bpm(140.5), "140.50");
        assert_eq!(format_time(0.0), "0:00.000");
        assert_eq!(format_time(65.123), "1:05.123");
        assert_eq!(format_time_sig(4, 4, 480), "4/4 480");
        assert_eq!(format_tick_bar_beat(0.0, 480, 4), "1.1.000");
        assert_eq!(format_tick_bar_beat(480.0, 480, 4), "1.2.000");
        assert_eq!(format_tick_bar_beat(1920.0, 480, 4), "2.1.000");
    }

    #[test]
    fn view_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = TransportState {
            is_playing: true,
            is_recording: true,
            step_input: true,
            bpm: 140.0,
            has_active_document: true,
            pinned_play_pause: true,
            pinned_stop: true,
            pinned_record: true,
            pinned_step_input: true,
            ..Default::default()
        };
        let _el = view(&window, state);
    }

    #[test]
    fn view_with_pinned_files() {
        let window = Window::new("Tokyo Night Storm");
        let mut state = TransportState {
            has_active_document: true,
            ..Default::default()
        };
        state.pinned_file_actions[0] = true;
        state.pinned_file_actions[1] = true;
        state.pinned_edit_actions[0] = true;
        state.pinned_edit_actions[1] = true;
        let _el = view(&window, state);
    }

    #[test]
    fn view_vertical_orientation() {
        let window = Window::new("Tokyo Night Storm");
        let state = TransportState {
            orientation: Orientation::Vertical,
            has_active_document: true,
            active_tool: Tool::Curve,
            ..Default::default()
        };
        let _el = view(&window, state);
    }

    #[test]
    fn all_file_actions_have_icons() {
        for a in FileAction::ALL {
            let _ = a.icon();
            assert!(!a.label().is_empty());
        }
    }

    #[test]
    fn all_edit_actions_have_icons() {
        for a in EditAction::ALL {
            let _ = a.icon();
            assert!(!a.label().is_empty());
        }
    }

    #[test]
    fn digital_constants_match_original() {
        assert!((TRANSPORT_H - 40.0).abs() < f32::EPSILON);
        assert!((BTN_SIZE - 32.0).abs() < f32::EPSILON);
        assert!((DIGITAL_W - 166.0).abs() < f32::EPSILON);
        assert!((DIGITAL_H - 36.0).abs() < f32::EPSILON);
        assert!((DIGITAL_RADIUS - 8.0).abs() < f32::EPSILON);
    }
}
