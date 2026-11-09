//! 音频导出对话框（`DialogType::AudioExport`）。
//!
//! 整窗内容底 + 内边距 24，六个区块自上而下：标题 18px、工程信息、
//! 音频设置、事件过滤、输出路径、底部按钮（未渲染 / 渲染中 / 完成三态），
//! 整体在 `ScrollArea` 内滚动（生产 `scrollable(main_content)`）。
//!
//! 区块实现分别在 `settings` / `filter` / `buttons`，控件助手在 `fields`；
//! 每个控件只回传原样文本或固定选项下标，业务校验与导出仍由生产侧负责。

mod buttons;
mod fields;
mod filter;
mod settings;

use egui::{Align, Id, RichText, ScrollArea, Vec2};

use crate::chrome::{AudioExportModel, AudioExportPanelModel, AudioToggleId};
use crate::theme;

const ACTION_ID: &str = "lumino.audio-export.action";
/// 整窗内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;

/// 音频导出面板产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioExportAction {
    /// 工程名称变化。
    ProjectNameChanged(String),
    /// 浏览 MIDI 文件。
    BrowseMidi,
    /// 浏览音色库。
    BrowseSoundfont,
    /// 输出路径文本变化。
    OutputPathChanged(String),
    /// 浏览输出路径。
    BrowseOutput,
    /// 渲染后端（选项表下标）。
    BackendChanged(usize),
    /// 输出格式（选项表下标）。
    FormatChanged(usize),
    /// 比特率文本变化。
    BitrateChanged(String),
    /// 采样率（选项表下标）。
    SampleRateChanged(usize),
    /// 通道数（选项表下标）。
    ChannelsChanged(usize),
    /// 每键复音文本变化。
    LayersChanged(String),
    /// 通道多线程（选项表下标）。
    ChannelThreadingChanged(usize),
    /// 按键多线程（选项表下标）。
    KeyThreadingChanged(usize),
    /// 插值算法（选项表下标）。
    InterpolationChanged(usize),
    /// 复选框（字段 + 新值）。
    Toggle(AudioToggleId, bool),
    /// 力度下限文本。
    VelocityLowChanged(String),
    /// 力度上限文本。
    VelocityHighChanged(String),
    /// 键位下限文本。
    KeyLowChanged(String),
    /// 键位上限文本。
    KeyHighChanged(String),
    /// 音符结束延迟文本。
    NoteForceEndDelayChanged(String),
    /// 导出。
    Confirm,
    /// 关闭面板。
    ClosePanel,
    /// 暂停 / 继续。
    TogglePause,
    /// 中止。
    Abort,
    /// 重置渲染状态。
    ResetRendering,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: AudioExportAction) {
    if ctx
        .data(|d| d.get_temp::<Option<AudioExportAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<AudioExportAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<AudioExportAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整块面板（`rect` = 生产里被整块替换的右侧内容区）。
///
/// 面板不透明地覆盖同位置的 iced 内容（工具栏 / 画布 / 状态栏），
/// 因此叠加层会在同一矩形上拦截点击，避免双层同时响应。
pub fn show_panel(ui: &mut egui::Ui, model: &AudioExportPanelModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.painter().rect_filled(model.rect, 0, theme::base_bg());
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(model.rect.shrink(PAD))
            .layout(egui::Layout::top_down(Align::Min)),
    );
    panel_content(&mut root, &model.content);
}

/// 面板正文（滚动区 + 六个区块），两个入口共用。
fn panel_content(child: &mut egui::Ui, model: &AudioExportModel) {
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = child.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();

    ScrollArea::vertical()
        .id_salt("lumino.audio-export.scroll")
        .auto_shrink([false, false])
        .show(child, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add(egui::Label::new(
                RichText::new(&model.title).size(18.0).color(theme::text()),
            ));
            ui.add_space(16.0);

            project(ui, model);
            ui.add_space(16.0);
            settings::show(ui, &model.settings);
            ui.add_space(16.0);
            filter::show(ui, &model.filter);
            ui.add_space(16.0);
            output(ui, model);
            ui.add_space(24.0);
            buttons::show(ui, &model.buttons);
        });
}

/// 工程信息：名称输入 + MIDI / 音色库只读路径 + 三个「浏览...」。
fn project(ui: &mut egui::Ui, model: &AudioExportModel) {
    let ctx = ui.ctx().clone();
    let block = &model.project;
    ui.add(egui::Label::new(
        RichText::new(&block.name_label)
            .size(theme::font::LABEL)
            .color(theme::text()),
    ));
    ui.add_space(4.0);
    fields::input(
        ui,
        "project-name",
        ui.available_width().max(1.0),
        &block.name,
        &block.name_hint,
        |text| push_action(&ctx, AudioExportAction::ProjectNameChanged(text)),
    );

    for (id, label, path, browse_action) in [
        (
            "midi-path",
            &block.midi_label,
            &block.midi_path,
            AudioExportAction::BrowseMidi,
        ),
        (
            "soundfont-path",
            &block.soundfont_label,
            &block.soundfont_path,
            AudioExportAction::BrowseSoundfont,
        ),
    ] {
        ui.add_space(12.0);
        ui.add(egui::Label::new(
            RichText::new(label)
                .size(theme::font::LABEL)
                .color(theme::text()),
        ));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let browse_w = 80.0;
            let path_w = (ui.available_width() - browse_w - 8.0).max(1.0);
            fields::readonly(ui, path_w, path);
            ui.add_space(8.0);
            if fields::button(ui, &block.browse, Some(browse_w)).clicked() {
                push_action(&ctx, browse_action.clone());
            }
        });
        let _ = id;
    }
}

/// 输出路径：区块标题 + 输入框 + 浏览。
fn output(ui: &mut egui::Ui, model: &AudioExportModel) {
    let ctx = ui.ctx().clone();
    let block = &model.output;
    fields::section_title(ui, &block.title);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let browse_w = 80.0;
        let path_w = (ui.available_width() - browse_w - 8.0).max(1.0);
        fields::input(
            ui,
            "output-path",
            path_w,
            &block.path,
            &block.path_hint,
            |text| push_action(&ctx, AudioExportAction::OutputPathChanged(text)),
        );
        ui.add_space(8.0);
        if fields::button(ui, &block.browse, Some(browse_w)).clicked() {
            push_action(&ctx, AudioExportAction::BrowseOutput);
        }
    });
}
