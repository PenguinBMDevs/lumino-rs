//! 右侧栏面板内容：模型镜像与动作翻译。
//!
//! 当前接管「图片转 MIDI」「素材库」「钢琴瀑布流」：参数文本缓冲与调色板下标
//! 全部来自生产 `RightSidebar.config`，动作原样回传 `Message::RightSidebar(...)`。
//! 钢琴瀑布流的内容是 `lumino-gfx` 离屏纹理，宿主注册成 egui 外部纹理后由
//! `RightSidebarPanelModel::PianoWaterfall` 引用。

use lumino_core::{tr, trf};
use lumino_extras::i18n::main_translations;
use lumino_message::{I2mConfigField, RightSidebarAction as SidebarAction};
use lumino_ui_egui::chrome::right_panel::RightSidebarAction as PanelAction;
use lumino_ui_egui::chrome::{
    I2mControlModel, I2mConvertModel, I2mField, I2mPanelModel, I2mRowModel, RightSidebarModel,
    RightSidebarPanelKind, RightSidebarPanelModel, RightSidebarRailItem, WATERFALL_TEXTURE_ID,
    WaterfallPanelModel,
};
use lumino_ui_egui::layout;

use crate::Message;
use crate::right_sidebar::{RightSidebarPanel, palette_algorithms};
use crate::root::Root;

mod materials;

/// 右侧栏模型：图标列 + 已接管的面板内容。
pub(super) fn model(
    root: &Root,
    titlebar_present: bool,
    shows_toolbar: bool,
) -> Option<RightSidebarModel> {
    if !root.right_sidebar_visible() {
        return None;
    }
    let active = match root.right_sidebar.active_panel {
        RightSidebarPanel::ImageToMidi => RightSidebarPanelKind::ImageToMidi,
        RightSidebarPanel::Materials => RightSidebarPanelKind::Materials,
        RightSidebarPanel::PianoWaterfall => RightSidebarPanelKind::PianoWaterfall,
    };
    Some(RightSidebarModel {
        panel_visible: root.right_sidebar.panel_visible,
        panel_width: root.right_sidebar.panel_width,
        active: root.right_sidebar.panel_visible.then_some(active),
        top_inset: (if titlebar_present {
            layout::TITLEBAR_H
        } else {
            0.0
        }) + if shows_toolbar {
            root.toolbar.height
        } else {
            0.0
        },
        resizing: root.right_sidebar.is_resizing,
        content: content(root),
        rail: rail_items(root.settings.display.language),
    })
}

impl Root {
    /// 右侧栏面板动作 → 生产 `Message`。
    pub(crate) fn right_sidebar_action_message(&self, action: &PanelAction) -> Option<Message> {
        action_message(action)
    }
}

/// 图标列三项（文案取 i18n，顺序与 iced `view.rs` 一致）。
fn rail_items(language: lumino_extras::i18n::Language) -> Vec<RightSidebarRailItem> {
    let t = main_translations(language);
    vec![
        RightSidebarRailItem {
            kind: RightSidebarPanelKind::ImageToMidi,
            label: t.tool_image_to_midi.to_string(),
        },
        RightSidebarRailItem {
            kind: RightSidebarPanelKind::Materials,
            label: t.material_library.to_string(),
        },
        RightSidebarRailItem {
            kind: RightSidebarPanelKind::PianoWaterfall,
            label: t.piano_waterfall.to_string(),
        },
    ]
}
/// 当前页面的右侧栏内容；未接管的面板返回 `None`（继续由 iced 绘制）。
pub(super) fn content(root: &Root) -> Option<RightSidebarPanelModel> {
    if !root.right_sidebar.panel_visible || !root.right_sidebar_visible() {
        return None;
    }
    match root.right_sidebar.active_panel {
        RightSidebarPanel::ImageToMidi => {
            Some(RightSidebarPanelModel::ImageToMidi(i2m_model(root)))
        }
        RightSidebarPanel::Materials => {
            Some(RightSidebarPanelModel::Materials(materials::model(root)))
        }
        RightSidebarPanel::PianoWaterfall => Some(RightSidebarPanelModel::PianoWaterfall(
            waterfall_model(root),
        )),
    }
}

/// 钢琴瀑布流面板模型：标题 / 占位文案 / 离屏纹理（已注册时才有 ID）。
fn waterfall_model(root: &Root) -> WaterfallPanelModel {
    let language = root.settings.display.language;
    let t = main_translations(language);
    let state = &root.right_sidebar.piano_waterfall;
    WaterfallPanelModel {
        title: trf!("{name}预览", name = t.piano_waterfall),
        empty_hint: tr!("（键盘渲染中或面板不可见）").to_string(),
        texture: state
            .waterfall_view
            .is_some()
            .then_some(WATERFALL_TEXTURE_ID),
    }
}

fn i2m_model(root: &Root) -> I2mPanelModel {
    let sidebar = &root.right_sidebar;
    let cfg = &sidebar.config;
    let palette_options: Vec<String> = palette_algorithms()
        .into_iter()
        .map(|(name, _)| name.to_owned())
        .collect();
    let selected = sidebar.selected_image_path.is_some();

    I2mPanelModel {
        title: tr!("图片转 MIDI").to_string(),
        select_image: tr!("选择图片文件").to_string(),
        selected_hint: selected.then(|| tr!("选择了一个图片文件").to_string()),
        config_title: tr!("转换参数").to_string(),
        rows: vec![
            I2mRowModel {
                label: tr!("Key 范围").to_string(),
                control: I2mControlModel::KeyRange {
                    start_field: I2mField::StartKey,
                    start_text: cfg.start_key_text.clone(),
                    end_field: I2mField::EndKey,
                    end_text: cfg.end_key_text.clone(),
                },
            },
            I2mRowModel {
                label: tr!("目标高度").to_string(),
                control: I2mControlModel::Input {
                    field: I2mField::TargetHeight,
                    text: cfg.target_height_text.clone(),
                },
            },
            I2mRowModel {
                label: tr!("每像素 tick").to_string(),
                control: I2mControlModel::Input {
                    field: I2mField::TicksPerPixel,
                    text: cfg.ticks_per_pixel_text.clone(),
                },
            },
            I2mRowModel {
                label: tr!("颜色数").to_string(),
                control: I2mControlModel::Input {
                    field: I2mField::ColorCount,
                    text: cfg.color_count_text.clone(),
                },
            },
            I2mRowModel {
                label: tr!("调色板").to_string(),
                control: I2mControlModel::Palette {
                    options: palette_options,
                    selected: cfg.palette_index,
                },
            },
        ],
        convert: selected.then(|| I2mConvertModel {
            label: if sidebar.converting {
                tr!("转换中...").to_string()
            } else {
                tr!("转换为 MIDI").to_string()
            },
            enabled: !sidebar.converting,
        }),
    }
}

/// 面板动作 → 生产 `Message`。
pub(super) fn action_message(action: &PanelAction) -> Option<Message> {
    if let Some(message) = materials::action_message(action) {
        return Some(message);
    }
    Some(match action {
        PanelAction::ResizeStarted => Message::RightSidebar(SidebarAction::ResizeDragStarted),
        PanelAction::ResizeEnded => Message::RightSidebar(SidebarAction::ResizeDragEnded),
        PanelAction::RailClicked(kind) => Message::RightSidebar(match kind {
            RightSidebarPanelKind::ImageToMidi => SidebarAction::ImageToMidiClicked,
            RightSidebarPanelKind::Materials => SidebarAction::MaterialLibraryClicked,
            RightSidebarPanelKind::PianoWaterfall => SidebarAction::PianoWaterfallClicked,
        }),
        PanelAction::SelectImageFile => Message::RightSidebar(SidebarAction::SelectImageFile),
        PanelAction::I2mConfigTextChanged { field, text } => {
            Message::RightSidebar(SidebarAction::I2mConfigTextChanged {
                field: config_field(*field),
                text: text.clone(),
            })
        }
        PanelAction::I2mPaletteChanged(index) => {
            Message::RightSidebar(SidebarAction::I2mPaletteChanged(*index))
        }
        PanelAction::ConvertClicked => Message::RightSidebar(SidebarAction::ConvertClicked),
        _ => return None,
    })
}

fn config_field(field: I2mField) -> I2mConfigField {
    match field {
        I2mField::StartKey => I2mConfigField::StartKey,
        I2mField::EndKey => I2mConfigField::EndKey,
        I2mField::TargetHeight => I2mConfigField::TargetHeight,
        I2mField::TicksPerPixel => I2mConfigField::TicksPerPixel,
        I2mField::ColorCount => I2mConfigField::ColorCount,
    }
}
