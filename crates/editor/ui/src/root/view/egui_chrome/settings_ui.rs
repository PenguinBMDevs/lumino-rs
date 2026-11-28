//! 设置「界面」页：模型构建（语言 / 主题卡片 / 字体 / 交互 / 自动滚动）。

use egui::Color32;
use lumino_core::i18n::{Language, SettingsTranslations, settings_translations};
use lumino_core::locale::LanguagePreference;
use lumino_core::storage::config::{NoteCountDisplay, SelectionBoxMode};
use lumino_core::tr;
use lumino_ui_core::Theme;
use lumino_ui_core::theme::{HIGH_CONTRAST_DISPLAY, hc_theme};
use lumino_ui_egui::chrome::{ThemeCardModel, UiField, UiPageModel, UiRow};

use crate::root::Root;
use crate::settings::SettingsPanel;

mod actions;

pub(super) use actions::action_message;

/// 构造界面页模型：按原顺序把各分区行追加进 `rows`，最后组装页模型。
pub(super) fn page(root: &Root) -> UiPageModel {
    let settings = &root.settings;
    let language = settings.display.language;
    let t = settings_translations(language);
    let mut rows = Vec::new();

    push_language_rows(&mut rows, settings, t);
    push_theme_rows(&mut rows, root, t);
    push_font_rows(&mut rows, settings, t);
    push_titlebar_rows(&mut rows, settings, t);
    push_note_count_row(&mut rows, settings, t);
    push_auto_scroll_rows(&mut rows, settings, t);
    push_interaction_rows(&mut rows, settings, t, language);
    push_slider_rows(&mut rows, settings, t);

    UiPageModel {
        title: t.ui_title.to_owned(),
        rows,
    }
}

/// 语言偏好下拉行。
fn push_language_rows(rows: &mut Vec<UiRow>, settings: &SettingsPanel, t: &SettingsTranslations) {
    // 语言偏好。
    let mut languages = vec![t.language_follow_system.to_owned()];
    languages.extend(Language::all().iter().map(ToString::to_string));
    let language_selected = match settings.display.language_preference {
        LanguagePreference::System => 0,
        LanguagePreference::Fixed(fixed) => Language::all()
            .iter()
            .position(|lang| *lang == fixed)
            .map(|index| index + 1)
            .unwrap_or(0),
    };
    rows.push(UiRow::Combo {
        field: UiField::Language,
        label: tr!("语言 / Language:").to_string(),
        hint: String::new(),
        options: languages,
        selected: language_selected,
        width: 200.0,
    });
}

/// 主题卡片行（高对比度 + 注册表全部主题）。
fn push_theme_rows(rows: &mut Vec<UiRow>, root: &Root, t: &SettingsTranslations) {
    // 主题卡片：高对比度 + 注册表全部主题。
    let current = root.window.theme.to_string();
    let mut cards = Vec::with_capacity(Theme::ALL.len() + 1);
    cards.push(card(
        t.high_contrast,
        HIGH_CONTRAST_DISPLAY,
        &hc_theme(),
        &current,
    ));
    cards.extend(Theme::ALL.iter().map(|theme| {
        let name = theme.to_string();
        card(&name, &name, theme, &current)
    }));
    rows.push(UiRow::ThemeCards {
        label: t.theme.to_owned(),
        cards,
    });
}

/// 字体相关行：字体名 / 路径 / 浏览 / 应用提示。
fn push_font_rows(rows: &mut Vec<UiRow>, settings: &SettingsPanel, t: &SettingsTranslations) {
    // 字体：系统字体名 / 自定义路径 / 浏览。
    let font_list = lumino_note_core::font_scanner::get_cached_fonts();
    let font_names: Vec<String> = font_list.iter().map(|font| font.name.clone()).collect();
    let current_font = &settings.editing.program_font_name;
    // 先按显示名精确匹配；失败再用解析后的文件路径反查，保证
    // `Microsoft YaHei` 这类别名 / 本地化名称也能选中正确条目，而不是
    // 静默显示列表第一项（旧行为会让用户以为字体已经切过去了）。
    let font_selected = font_list
        .iter()
        .position(|font| font.name.eq_ignore_ascii_case(current_font))
        .or_else(|| {
            let resolved = lumino_note_core::font_scanner::resolve_font_file(current_font)?;
            font_list.iter().position(|font| font.path == resolved.path)
        })
        .unwrap_or(0);
    rows.push(UiRow::Combo {
        field: UiField::ProgramFontName,
        label: t.program_font.to_owned(),
        hint: t.font_hint.to_owned(),
        options: font_names,
        selected: font_selected,
        width: 200.0,
    });
    rows.push(UiRow::Text {
        field: UiField::ProgramFontPath,
        label: String::new(),
        hint: String::new(),
        value: settings.editing.program_font_path.clone(),
        placeholder: t.font_path_placeholder.to_owned(),
        width: 250.0,
        suffix: String::new(),
    });
    rows.push(UiRow::Button {
        field: UiField::BrowseProgramFont,
        label: t.browse.to_owned(),
    });
    rows.push(UiRow::Static {
        text: t.font_apply_hint.to_owned(),
    });
}

/// 原生标题栏与图标高清开关行。
fn push_titlebar_rows(rows: &mut Vec<UiRow>, settings: &SettingsPanel, t: &SettingsTranslations) {
    // 原生标题栏（macOS 不需要）。
    if !cfg!(target_os = "macos") {
        rows.push(UiRow::Checkbox {
            field: UiField::NativeTitlebar,
            label: t.native_titlebar.to_owned(),
            hint: t.native_titlebar_hint.to_owned(),
            checked: settings.synth.use_native_titlebar,
        });
    }
    rows.push(UiRow::Checkbox {
        field: UiField::IconHidpi,
        label: t.hidpi_icon.to_owned(),
        hint: t.hidpi_icon_hint.to_owned(),
        checked: settings.display.icon_hidpi,
    });
}

/// 音符总量显示位置下拉行。
fn push_note_count_row(rows: &mut Vec<UiRow>, settings: &SettingsPanel, t: &SettingsTranslations) {
    // 音符总量显示位置。
    let note_options = vec![
        t.note_count_display_project_settings.to_owned(),
        t.note_count_display_statusbar.to_owned(),
    ];
    let note_selected =
        usize::from(settings.display.note_count_display == NoteCountDisplay::StatusBar);
    rows.push(UiRow::Combo {
        field: UiField::NoteCountDisplay,
        label: t.note_count_display.to_owned(),
        hint: t.note_count_display_hint.to_owned(),
        options: note_options,
        selected: note_selected,
        width: 200.0,
    });
}

/// 自动滚动分区及其三个数值输入行。
fn push_auto_scroll_rows(
    rows: &mut Vec<UiRow>,
    settings: &SettingsPanel,
    t: &SettingsTranslations,
) {
    // 自动滚动。
    rows.push(UiRow::Section {
        title: t.auto_scroll.to_owned(),
        hint: t.auto_scroll_hint.to_owned(),
    });
    rows.push(UiRow::Text {
        field: UiField::AutoScrollFixed,
        label: t.auto_scroll_fixed.to_owned(),
        hint: String::new(),
        value: settings.auto_scroll.fixed_position.to_string(),
        placeholder: t.pixel.to_owned(),
        width: 80.0,
        suffix: t.from_left.to_owned(),
    });
    rows.push(UiRow::Text {
        field: UiField::AutoScrollTrigger,
        label: t.auto_scroll_trigger.to_owned(),
        hint: String::new(),
        value: settings.auto_scroll.page_trigger_offset.to_string(),
        placeholder: t.pixel.to_owned(),
        width: 80.0,
        suffix: t.from_right.to_owned(),
    });
    rows.push(UiRow::Text {
        field: UiField::AutoScrollReturn,
        label: t.auto_scroll_return.to_owned(),
        hint: String::new(),
        value: settings.auto_scroll.page_return_position.to_string(),
        placeholder: t.pixel.to_owned(),
        width: 80.0,
        suffix: t.from_left.to_owned(),
    });
}

/// 交互分区：选择框模式 / 钢琴键盘开关 / 默认第一屏。
fn push_interaction_rows(
    rows: &mut Vec<UiRow>,
    settings: &SettingsPanel,
    t: &SettingsTranslations,
    language: Language,
) {
    // 交互。
    rows.push(UiRow::Section {
        title: t.interaction.to_owned(),
        hint: String::new(),
    });
    let selection_modes = [SelectionBoxMode::Direct, SelectionBoxMode::Spring];
    let selection_selected = selection_modes
        .iter()
        .position(|mode| *mode == settings.editing.selection_box_mode)
        .unwrap_or(0);
    rows.push(UiRow::Combo {
        field: UiField::SelectionBoxMode,
        label: t.selection_box_mode.to_owned(),
        hint: t.selection_box_hint.to_owned(),
        options: selection_modes
            .iter()
            .map(|mode| lumino_core::i18n::selection_box_mode_name(*mode, language).to_owned())
            .collect(),
        selected: selection_selected,
        width: 200.0,
    });
    rows.push(UiRow::Section {
        title: t.piano_roll.to_owned(),
        hint: String::new(),
    });
    rows.push(UiRow::Checkbox {
        field: UiField::Enable256Key,
        label: t.enable_256key.to_owned(),
        hint: t.enable_256key_hint.to_owned(),
        checked: settings.display.enable_256key,
    });
    rows.push(UiRow::Checkbox {
        field: UiField::VelocityCurveStyle,
        label: tr!("力度面板曲线显示（默认）").to_string(),
        hint: tr!("关闭后使用柱状图显示力度值").to_string(),
        checked: settings.display.velocity_curve_style,
    });
    rows.push(UiRow::Checkbox {
        field: UiField::PlaybackKeyColors,
        label: tr!("播放时键盘颜色指示（默认关闭）").to_string(),
        hint: tr!("开启后播放时在钢琴键盘上高亮当前音符位置，占用额外内存").to_string(),
        checked: settings.display.playback_key_colors_enabled,
    });
    rows.push(UiRow::Section {
        title: tr!("默认第一屏").to_string(),
        hint: tr!("主区域没有打开任何面板时显示品牌占位").to_string(),
    });
    rows.push(UiRow::Slider {
        field: UiField::FirstScreenOpacity,
        label: tr!("不透明度").to_string(),
        hint: tr!("品牌占位图标与字标的显示透明度（默认 20%）").to_string(),
        value: settings.display.first_screen_opacity,
        min: 0.0,
        max: 1.0,
        step: 0.01,
        value_text: format!("{:.0}%", settings.display.first_screen_opacity * 100.0),
    });
}

/// 自动化曲线粗细与监控刷新间隔滑杆。
fn push_slider_rows(rows: &mut Vec<UiRow>, settings: &SettingsPanel, t: &SettingsTranslations) {
    // 自动化曲线粗细 / 监控刷新间隔。
    rows.push(UiRow::Slider {
        field: UiField::AutomationLineThickness,
        label: t.ui_automation_line_thickness.to_owned(),
        hint: t.ui_automation_line_thickness_hint.to_owned(),
        value: settings.editing.automation_line_thickness,
        min: 1.0,
        max: 10.0,
        step: 0.5,
        value_text: format!("{:.1} px", settings.editing.automation_line_thickness),
    });
    rows.push(UiRow::Slider {
        field: UiField::MonitorRefreshInterval,
        label: t.ui_monitor_refresh_interval.to_owned(),
        hint: t.ui_monitor_refresh_interval_hint.to_owned(),
        value: settings.logging.monitor_refresh_interval_ms,
        min: 50.0,
        max: 2000.0,
        step: 1.0,
        value_text: format!("{:.0} ms", settings.logging.monitor_refresh_interval_ms),
    });
}

fn card(display: &str, value: &str, theme: &Theme, current: &str) -> ThemeCardModel {
    let palette = theme.extended_palette();
    let bg = color32(palette.background.base.color);
    let title = color32(palette.background.base.text);
    let accent = color32(palette.primary.base.color);
    let control = derive_surface(bg, title, 0.05, 0.03);
    let line = derive_surface(bg, title, 0.18, 0.07);
    ThemeCardModel {
        display: display.to_owned(),
        value: value.to_owned(),
        selected: value == current,
        bg,
        title,
        control,
        bar_primary: title,
        bar_secondary: derive_secondary_text(bg, title),
        accent,
        line,
        line_soft: Color32::from_rgba_unmultiplied(line.r(), line.g(), line.b(), 110),
    }
}

fn color32(color: lumino_ui_core::geom::Color) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
        (color.a * 255.0).round() as u8,
    )
}

fn luminance(color: Color32) -> f32 {
    0.299 * f32::from(color.r()) / 255.0
        + 0.587 * f32::from(color.g()) / 255.0
        + 0.114 * f32::from(color.b()) / 255.0
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
        255,
    )
}

fn derive_surface(bg: Color32, text: Color32, dark: f32, light: f32) -> Color32 {
    if luminance(bg) <= 0.5 {
        mix(bg, text, dark)
    } else {
        mix(bg, Color32::BLACK, light)
    }
}

fn derive_secondary_text(bg: Color32, text: Color32) -> Color32 {
    if luminance(bg) <= 0.5 {
        mix(text, Color32::BLACK, 0.18)
    } else {
        mix(text, bg, 0.14)
    }
}
