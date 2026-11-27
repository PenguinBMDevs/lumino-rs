//! 旧版 i18n 结构体的兼容层（T-34b）。
//!
//! `MainTranslations` / `SettingsTranslations` 曾由 `lumino-extras` 维护两份
//! 中英文静态表；现在所有词条只存在于 [`super::catalog`]，本模块仅在**调用点
//! 迁移完成前**提供同名字段，值由 [`super::tr_in`] 从 catalog 取。
//!
//! - 禁止在本文件新增中英文对照：字段只能映射到 catalog 已登记的原文 key；
//! - 新增文案请直接写 `tr!` / `trf!`，不要再扩这两个结构体；
//! - 字段调用点全部改 `tr_in` 后可整文件删除；当前保留只为类型化设置页标签。
//!
//! 结构体字段很宽（156 / 153），因为它们是历史 API 的逐字段镜像；这是纯 DTO，
//! 不承载行为，宽是兼容成本而不是设计选择。
//!
//! 首次取词约 2 ms（debug）/ 亚毫秒（release），每进程每语言一次，之后零查找；
//! 不在音频线程上调用。

use std::sync::OnceLock;

use super::{Language, tr_in};

/// 用一张「字段 => 中文原文」表同时生成结构体、取词函数与 key 清单。
///
/// T-34b 初版把同一份 key 写了三遍：字段声明、`tr_in` 调用、测试用的
/// `LEGACY_KEYS`。三者靠人工同步，已经漏过一条（`app_name`）。宏把三者合成
/// 一次展开，漂移从「测试才发现」变成「编译不过」：
///
/// - 少写字段：结构体字面量缺字段，编译失败；
/// - 多写字段：结构体没有该字段，编译失败；
/// - key 改动：`ALL_KEYS` 与实际取词来自同一个字面量，不可能不一致。
macro_rules! translations {
    ($(#[$meta:meta])* $name:ident => [$($field:ident => $zh:literal),+ $(,)?]) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            $(pub $field: &'static str,)+
        }

        impl $name {
            /// 首次构造时从 catalog 取词，之后由 `OnceLock` 缓存。
            fn build(lang: Language) -> Self {
                Self { $($field: tr_in($zh, lang),)+ }
            }

            /// 本结构体引用的全部中文原文，供 catalog 覆盖率测试遍历。
            pub const ALL_KEYS: &'static [&'static str] = &[$($zh,)+];
        }
    };
}

translations! {
    /// 主界面文案（兼容旧 `lumino_extras::i18n::MainTranslations`）
    MainTranslations => [
        skip_backward => "快退",
        pause => "暂停",
        play => "播放",
        skip_forward => "快进",
        loop_on => "循环播放: 开",
        loop_off => "循环播放: 关",
        toggle_loop_tooltip => "切换循环播放",
        record_start => "开始录制",
        record_stop => "停止录制",
        tool_pointer => "选择工具",
        tool_pointer_y_select => "Y向框选工具",
        tool_pencil => "铅笔工具",
        tool_eraser => "橡皮擦",
        tool_curve => "曲线工具",
        tool_fill => "颜料桶",
        tool_panel_tooltip => "音符画工具箱",
        tool_panel_settings => "音符画设置",
        draw_settings_placeholder => "设置项开发中：后续在此聚合描边、画笔、形状与颜料桶等绘制参数。",
        tool_mouse => "鼠标工具",
        tool_thickness => "粗细设置",
        draw_thickness_unit => "层",
        draw_thickness_hint => "全局：曲线/形状描边与笔画都按它沿线段法线加粗；画布线宽 = 层数 × 单音高行高（随缩放）",
        tool_brush => "自由笔画",
        tool_shape => "形状工具",
        tool_text => "文字输入",
        tool_curve_desc => "曲线工具：绘制连续音符音高曲线，可叠加颜料桶填充",
        tool_fill_desc => "颜料桶：填充封闭区域；可随时切换，仅作用于曲线/形状绘制的封闭图形",
        tool_brush_desc => "自由笔画：拖出矢量笔画（实时预览），松手后 √ 一次性生成音符；独立工具，不可与颜料桶共存",
        tool_shape_desc => "形状工具：绘制矩形/椭圆等形状；可与颜料桶共存",
        tool_text_desc => "文字输入：在钢琴卷帘中插入文本标注；独立工具",
        tool_eraser_desc => "橡皮擦：擦除音符；独立工具",
        tool_mouse_desc => "鼠标工具：点选已绘制的图形，选中后高亮描边",
        tool_quantize => "量化",
        tool_speed => "变速",
        tool_flip_vertical => "垂直翻转",
        tool_flip_horizontal => "水平翻转",
        tool_split => "分割(Split)",
        tool_glue => "合并(Glue)",
        tool_tie => "连奏(Tie)",
        tool_transpose_down => "移调 -1",
        tool_transpose_up => "移调 +1",
        tool_transpose_down_octave => "低八度",
        tool_transpose_up_octave => "高八度",
        precision_label => "精度:",
        precision_placeholder => "选择精度",
        auto_scroll_fixed => "自动滚动: 固定",
        auto_scroll_scrolling => "自动滚动: 滚动",
        auto_scroll_off => "自动滚动: 关闭",
        auto_scroll_tooltip => "切换自动滚动模式",
        onion_skin => "洋葱皮",
        onion_skin_tooltip => "切换洋葱皮显示",
        collaboration_label => "多人协作",
        collaboration_tooltip => "打开协作面板",
        toolbar_more => "更多工具",
        tool_image_to_midi => "图片转MIDI",
        undo => "撤销",
        redo => "重做",
        menu_file => "文件",
        menu_edit => "编辑",
        menu_view => "视图",
        menu_help => "帮助",
        file_new => "新建",
        file_open => "打开",
        file_save => "保存",
        file_close => "关闭",
        file_recent => "最近打开",
        file_recent_clear => "清除最近记录",
        file_recent_empty => "暂无最近文件",
        file_import => "导入文件",
        file_import_from_cloud => "从云导入",
        file_save_to_cloud => "保存到云",
        file_export_project => "导出工程",
        file_export_archive => "导出为单文件",
        file_export_folder => "导出为文件夹",
        file_export_material => "导出为素材",
        file_export_midi => "导出为MIDI",
        file_export_audio => "导出音频",
        file_project_settings => "工程设置",
        file_settings => "设置",
        file_exit => "退出",
        edit_undo => "撤销",
        edit_redo => "重做",
        edit_cut => "剪切",
        edit_copy => "复制",
        edit_paste => "粘贴",
        edit_select_all => "全选",
        edit_find => "查找",
        view_zoom_in => "放大",
        view_zoom_out => "缩小",
        view_zoom_reset => "重置缩放",
        help_about => "关于",
        first_screen_new_project => "新建工程",
        first_screen_open_file => "打开工程文件",
        first_screen_open_folder => "打开工程文件夹",
        first_screen_settings => "打开设置",
        first_screen_collaboration => "连接到多人协作房间",
        mode_editor => "编辑器",
        mode_waterfall => "瀑布流",
        mode_switch_to_editor => "切换到编辑器模式",
        mode_switch_to_waterfall => "切换到瀑布流模式",
        status_ready => "就绪",
        note_count_label => "音符总量",
        status_file_saved => "文件已经保存",
        status_save_failed => "保存失败",
        status_midi_exported => "MIDI 已导出",
        status_no_midi_content => "没有可导出的 MIDI 内容",
        sidebar_file => "音轨列表",
        sidebar_arrangement => "音轨总览",
        sidebar_automation => "自动化面板",
        sidebar_track_list => "音轨列表",
        sidebar_add_track => "添加音轨",
        precision_note_label => "分音符",
        precision_divide_by => "除以",
        precision_ok => "确定",
        precision_cancel => "取消",
        fill_division_title => "分音符填充",
        fill_division_prefix => "使用",
        fill_division_suffix => "分音符填充",
        fill_division_hint => "留空 = 整块填充（不切分）",
        fill_division_disable => "关闭填充",
        velocity_panel_velocity => "力度",
        velocity_panel_tempo => "速度",
        velocity_panel_velocity_info => "力度 0-127",
        velocity_panel_tempo_info => "速度 BPM",
        project_title => "工程信息设置",
        project_name_label => "项目名称",
        project_name_placeholder => "输入项目名称（留空显示为'无标题'）",
        project_bpm_label => "BPM 速度",
        project_copyright_label => "版权信息",
        project_copyright_placeholder => "输入版权信息（可选）",
        project_author_label => "作者",
        project_author_placeholder => "输入作者（可选）",
        project_created_label => "创建日期",
        project_unknown => "未知",
        project_editing_time_label => "累计创作时间",
        project_ok => "确定",
        project_cancel => "取消",
        material_library => "素材库",
        piano_waterfall => "钢琴瀑布流",
        material_add => "添加素材",
        material_download_web => "从云导入",
        material_import_local => "从本地选取",
        material_section_builtin => "内置素材",
        material_section_user => "本地素材",
        material_name_label => "名称：",
        material_author_label => "作者：",
        material_location_label => "位置：",
        material_track_label => "轨道数：",
        material_source_label => "来源：",
        material_invalid => "素材无效",
        material_import_ok => "素材已导入",
        material_import_failed => "素材导入失败",
        material_delete_title => "删除素材",
        material_delete_warning => "删除后无法恢复，确定要删除吗？",
        material_delete => "删除",
        material_delete_cancel => "取消",
    ]
}

translations! {
    /// 设置界面文案（兼容旧 `lumino_extras::i18n::SettingsTranslations`）
    SettingsTranslations => [
        general => "常规",
        audio => "音频",
        ui => "界面",
        shortcuts => "快捷键",
        about => "关于",
        onion_skin => "洋葱皮",
        palette => "调色板",
        editing => "编辑",
        cloud_manage => "云管理",
        confirm => "确认",
        cancel => "取消",
        ok => "确定",
        or => "或",
        browse => "浏览...",
        general_title => "常规",
        eraser_behavior => "橡皮擦行为:",
        eraser_default_hint => "默认: Shift+拖动框选删除，点击删除单个",
        eraser_direct_hint => "直接框选: 拖动框选删除，Shift+点击删除单个",
        track_add_behavior => "添加音轨时:",
        track_add_behavior_hint => "选择添加音轨后是否自动跳转到新音轨",
        audio_title => "音频",
        synthesizer => "合成器:",
        soundfont => "音色库:",
        soundfont_placeholder => "选择音色库文件 (SFZ/SF2)...",
        buffer_latency => "缓冲区 (延迟)",
        max_voices => "每键最大同音数:",
        max_voices_hint => "同键快速重复/密集和弦时，提高此值减少 voice stealing 导致的断音",
        global_voice_limit => "全局最大复音数:",
        global_voice_limit_auto => "自动",
        global_voice_limit_hint => "引擎的复音硬上限（量程）。实际运行目标按下方比例留出暂态余量，负载反馈只会更低、不会更高；超限时从最忙通道抢占最老/最轻声部，新音符优先保留",
        voice_target_ratio => "复音软目标比例:",
        voice_target_ratio_hint => "运行目标 = 比例 × 硬上限（默认 1-1/e≈0.632，留约 37% 暂态余量；1-1/e²≈0.865 更激进但余量仅 13.5%）",
        soft_nps_gate => "过载保命闸（仅在重度过载时临时限速，默认关闭）",
        soft_nps_gate_hint => "关闭时不存在任何 NoteOn 丢弃路径；开启后仅在负载持续超限时短暂限速，回落后自动解除",
        lgs_nps_gate => "防爆闸（LGS，默认开启）",
        lgs_nps_gate_hint => "开启时：过载（L2+）自动对 NoteOn 令牌桶限速（突发 50ms 成组放行，被拒音符的 NoteOff 配对抵消），保连续但会变薄；关闭时：完全放行，过载会周期性静音",
        velocity_filter => "力度过滤阈值",
        velocity_filter_hint => "力度小于等于阈值的音符将不播放（0=关闭过滤）",
        midi_input_device => "MIDI 输入设备:",
        no_device => "无可用设备",
        select_device_placeholder => "选择MIDI设备",
        kdmapi_hint => "KDMAPI 模式使用系统驱动，无需音色库",
        system_hint => "System 模式使用系统默认的WinMM MIDI输出，无需音色库",
        winmm_output_device => "WinMM 输出设备 (播表):",
        winmm_no_device => "未检测到可用的 WinMM 输出设备",
        refresh => "刷新",
        xsynth_hint => "XSynth: 内置高性能合成器，支持SFZ/SF2格式音色库",
        lgs_hint => "LGS (GPU): 基于 GPU 加速的渲染合成器，使用 lumino-gpu-synth 管线",
        builtin_engine => "内置合成器引擎:",
        lgs_buffer => "缓冲区大小 (samples):",
        lgs_buffer_hint => "GPU 一次调度的音频帧数，须为 2 的幂（64–8192）；越小延迟越低但密集段落 GPU 开销越大",
        lgs_offline_buffer => "离线导出块长 (samples):",
        lgs_offline_buffer_hint => "只影响离线导出的 GPU 块长（512–8192，2 的幂）；越大渲染越快，但每块内存与尾延迟更高；实时播放不受影响",
        gpu_buffer_status => "状态栏显示 GPU 缓冲状态（默认开启）",
        gpu_buffer_status_hint => "在底边栏显示 LGS (GPU) 的队列深度（秒+趋势箭头）、渲染负载、活跃声部与欠载计数；关闭后整组隐藏",
        gpu_governor_metrics => "显示防爆治理指标（GOV/DROP）",
        gpu_governor_metrics_hint => "在 GPU 指标组内附加显示防爆治理级别与丢弃计数（仅异常时出现）",
        gpu_load_display => "负载显示口径:",
        gpu_load_display_hint => "LOAD 指标的显示方式：3 秒平均（稳定，默认）或瞬时（最近一块，抓尖峰，数值变化快）",
        audio_output_device => "音频播放输出设备:",
        audio_output_default => "系统默认输出设备",
        audio_output_no_device => "未检测到可用的音频播放输出设备",
        audio_output_hint => "选择音频播放输出设备；默认使用系统默认设备。更改后下次播放（软件合成器重建）生效",
        ui_title => "界面",
        theme => "主题:",
        program_font => "程序字体:",
        font_path_placeholder => "或输入字体文件路径...",
        font_hint => "系统字体与自定义字体文件二选一（文件路径优先）。字体加载失败时回退默认字体；修改后立即生效。",
        font_apply_hint => "字体修改将立即生效",
        native_titlebar => "使用经典系统标题栏",
        native_titlebar_hint => "启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮",
        hidpi_icon => "启用 HiDPI 图标渲染（推荐）",
        hidpi_icon_hint => "开启后图标以2x分辨率渲染，在视网膜屏幕上更清晰。关闭可节省少量内存和渲染开销。",
        auto_scroll => "自动滚动设置",
        auto_scroll_fixed => "模式1 - 指示线固定位置:",
        auto_scroll_trigger => "模式2 - 翻页触发位置:",
        auto_scroll_return => "模式2 - 翻页后位置:",
        auto_scroll_hint => "设置卷帘自动滚动时演奏指示线的位置行为",
        interaction => "交互",
        selection_box_mode => "框选框模式:",
        selection_box_hint => "直接跟随：框选框实时跟随鼠标，响应最即时。弹簧动画：框选框边界带有弹性动画效果，视觉更生动。",
        piano_roll => "钢琴卷帘",
        enable_256key => "启用 256 键扩展钢琴卷帘",
        enable_256key_hint => "开启后钢琴卷帘拓展至 256 键 (0-255)，扩展区域（128-255）颜色略深以便区分。需要较强的 GPU 性能。",
        note_count_display => "音符总量显示位置",
        note_count_display_hint => "选择音符总量的展示位置：工程设置对话框中，或下边栏（空闲时顶替「就绪」；悬停解释与状态消息优先显示）。",
        note_count_display_project_settings => "工程设置面板",
        note_count_display_statusbar => "下边栏",
        pixel => "像素",
        from_left => "像素 (从左边缘算起)",
        from_right => "像素 (从右边缘算起)",
        shortcuts_title => "快捷键",
        shortcuts_placeholder => "快捷键设置内容",
        about_title => "关于",
        app_name => "Lumino",
        version => "版本",
        app_description => "一个高效的MIDI编辑工具",
        high_contrast => "高对比度",
        palette_title => "调色板",
        palette_select => "选择调色板:",
        palette_hint => "调色板文件存放在 resources/palettes/ 目录下，编译时自动检测。添加新的 PNG 文件后重新编译即可使用。",
        palette_colors_info => "颜色预览",
        palette_no_preview => "无法预览选中调色板",
        palette_locked => "已加载，调色板锁定",
        editing_title => "编辑",
        editing_history_section => "操作历史",
        editing_history_total_limit => "操作日志总条数上限:",
        editing_history_total_limit_hint => "超过此值时弹出最早的日志（默认 100，建议 50-200）",
        editing_history_entry_limit => "单条日志条目上限:",
        editing_history_entry_limit_hint => "超过此值时强制分割为新日志（默认 1000，建议 500-2000）",
        editing_merge_window => "合并窗口（毫秒）:",
        editing_merge_window_hint => "仅 Pencil 连续绘制：在窗口内连续放置的音符合并为一个撤销日志（0=不合并，默认 300）",
        editing_intercept_section => "编辑拦截",
        editing_intercept_notification => "拦截时显示 Toast 提示",
        editing_intercept_notification_hint => "编辑中触发 Undo/Redo/Save/Play/Export 时显示提示（关闭则静默处理）",
        editing_autosave_section => "自动保存",
        editing_autosave_enabled => "启用周期自动保存",
        editing_autosave_enabled_hint => "按设定间隔把当前工程写入独立的自动保存文件（不影响手动保存的 .lmpj）。异常退出后下次启动可询问恢复；正常退出时自动清理。",
        editing_autosave_interval => "自动保存间隔（分钟）:",
        editing_autosave_interval_hint => "两次自动保存的最小间隔（1-120，默认 5）。仅工程有未保存更改时才会写入。",
        editing_tempo_max_bpm => "Tempo BPM 上限:",
        editing_tempo_max_bpm_hint => "Tempo 面板 BPM 绘制上限（默认 512）。速度点与刻度线按此范围映射，上限越大曲线越平坦。",
        editing_tempo_custom_option => "自定义",
        editing_tempo_custom_title => "自定义 BPM 上限",
        editing_tempo_custom_placeholder => "输入 BPM 上限（如 700）",
        ui_automation_line_thickness => "自动化曲线连线粗细:",
        ui_automation_line_thickness_hint => "自动化面板中事件瞄点之间的连线粗细（1-10 像素）",
        ui_monitor_refresh_interval => "监控数据刷新间隔:",
        ui_monitor_refresh_interval_hint => "底边栏 CPU/MEM/FPS 监控数据的刷新频率（50-2000ms，默认 100ms）",
        language_follow_system => "跟随系统",
        log_retention_section => "日志",
        log_retention_count => "日志文件保留份数:",
        log_retention_count_hint => "日志文件存储在配置目录的 logs/ 下，超过此份数时自动删除最旧的日志（0 = 不限制）",
        compatibility => "兼容性",
        compatibility_title => "兼容性",
        compat_check_button => "检查 GPU 兼容性",
        compat_checking => "检查中…",
        compat_never_checked => "尚未执行检测",
        compat_passed => "检测通过",
        compat_failed => "检测失败",
        compat_timeout => "检测超时（驱动可能挂起）",
        compat_timeout_hint => "检测未在时限内完成，驱动可能仍在后台挂起；若界面持续卡顿请重启应用。",
        compat_retry_cooling => "冷却中，请稍候再试…",
        compat_retry_locked => "检测已连续超时，本次运行不再重试；请重启应用后再检查。",
        compat_copy_diagnostics => "复制诊断信息",
        compat_copied => "已复制",
        compat_check_on_startup => "每次启动时检查 GPU 兼容性",
        compat_check_on_startup_hint => "关闭后不再执行启动检测；开启可用于诊断启动异常（防止死锁）",
        compat_show_warning => "启动时显示 GPU 兼容性警告",
        compat_show_warning_hint => "关闭后检测仍会执行，失败时仅在状态栏与日志中提示，不弹出警告窗",
        compat_domino_clipboard => "与 Domino 互通剪贴板",
        compat_domino_clipboard_hint => "开启后复制会额外写入 Domino(MidiPortalSequence) 格式，粘贴可解析 Domino 数据；大选区复制会额外耗时并占用内存",
        compat_domino_clipboard_unsupported_hint => "仅 Windows 生效（当前平台无 Domino 剪贴板互通实现）",
    ]
}

static ZH_MAIN: OnceLock<MainTranslations> = OnceLock::new();
static EN_MAIN: OnceLock<MainTranslations> = OnceLock::new();
static ZH_SETTINGS: OnceLock<SettingsTranslations> = OnceLock::new();
static EN_SETTINGS: OnceLock<SettingsTranslations> = OnceLock::new();

/// 取主界面文案（按语言缓存，首次调用后零查找）。
pub fn main_translations(lang: Language) -> &'static MainTranslations {
    match lang {
        Language::ZhCn => ZH_MAIN.get_or_init(|| MainTranslations::build(Language::ZhCn)),
        Language::EnUs => EN_MAIN.get_or_init(|| MainTranslations::build(Language::EnUs)),
    }
}

/// 取设置界面文案（按语言缓存）。
pub fn settings_translations(lang: Language) -> &'static SettingsTranslations {
    match lang {
        Language::ZhCn => ZH_SETTINGS.get_or_init(|| SettingsTranslations::build(Language::ZhCn)),
        Language::EnUs => EN_SETTINGS.get_or_init(|| SettingsTranslations::build(Language::EnUs)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原文与英文相同、按设计不进 catalog 的词条：catalog 拒绝 `zh == en`。
    const IDENTITY_KEYS: &[&str] = &["Lumino"];

    fn all_keys() -> impl Iterator<Item = &'static str> {
        MainTranslations::ALL_KEYS
            .iter()
            .chain(SettingsTranslations::ALL_KEYS.iter())
            .copied()
    }

    /// 兼容层引用的每条原文都必须能在 catalog 找到英文；缺失即迁移遗漏。
    ///
    /// 该清单来自宏展开，与字段一一对应，不需要手工维护。
    #[test]
    fn test_legacy_keys_registered() {
        let missing: Vec<_> = all_keys()
            .filter(|key| !IDENTITY_KEYS.contains(key))
            .filter(|key| super::super::lookup_en(key).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "兼容层有 {} 条 key 未登记英文：{:?}",
            missing.len(),
            missing
        );
    }

    /// 例外清单不能腐烂：登记的每条必须真的在字段表里，且真的没有英译。
    #[test]
    fn test_identity_keys_stay_untranslated() {
        for key in IDENTITY_KEYS {
            assert!(
                all_keys().any(|k| k == *key),
                "{key} 已不在兼容层字段表中，请删除这条例外"
            );
            assert!(
                super::super::lookup_en(key).is_none(),
                "{key} 已登记英译，不应再留在例外清单"
            );
        }
    }

    /// 中文取值必须与原文一致（兼容层不得偷偷改写中文）。
    #[test]
    fn test_legacy_chinese_text_is_source() {
        let t = main_translations(Language::ZhCn);
        assert_eq!(t.play, "播放");
        assert_eq!(t.pause, "暂停");
        let s = settings_translations(Language::ZhCn);
        assert_eq!(s.ok, "确定");
    }

    /// 英译不得整体退化为中文：抽查长文案与短标签各一条。
    #[test]
    fn test_english_translation_is_not_chinese() {
        let t = main_translations(Language::EnUs);
        assert_eq!(t.play, "Play");
        assert_ne!(t.play, "播放");
        let s = settings_translations(Language::EnUs);
        assert_eq!(s.ok, "OK");
        assert_ne!(s.app_description, "一个高效的MIDI编辑工具");
    }
}
