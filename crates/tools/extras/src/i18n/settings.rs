//! 设置面板翻译

mod enus;
mod zhcn;

use super::Language;
use enus::ENUS_SETTINGS;
use zhcn::ZHCN_SETTINGS;

/// 设置面板翻译
#[derive(Debug, Clone)]
pub struct SettingsTranslations {
    // ── 菜单项 ──
    /// 常规菜单项
    pub general: &'static str,
    /// 音频菜单项
    pub audio: &'static str,
    /// 界面菜单项
    pub ui: &'static str,
    /// 快捷键菜单项
    pub shortcuts: &'static str,
    /// 关于菜单项
    pub about: &'static str,
    /// 洋葱皮菜单项
    pub onion_skin: &'static str,
    /// 调色板菜单项
    pub palette: &'static str,
    /// 编辑菜单项
    pub editing: &'static str,
    /// 云管理菜单项
    pub cloud_manage: &'static str,

    // ── 通用 ──
    /// 确认
    pub confirm: &'static str,
    /// 取消
    pub cancel: &'static str,
    /// 确定
    pub ok: &'static str,
    /// 或
    pub or: &'static str,
    /// 浏览...
    pub browse: &'static str,

    // ── 常规页面 ──
    /// 常规页面标题
    pub general_title: &'static str,
    /// 橡皮擦行为
    pub eraser_behavior: &'static str,
    /// 橡皮擦默认模式提示
    pub eraser_default_hint: &'static str,
    /// 橡皮擦直接框选模式提示
    pub eraser_direct_hint: &'static str,
    /// 添加音轨行为
    pub track_add_behavior: &'static str,
    /// 添加音轨行为提示
    pub track_add_behavior_hint: &'static str,

    // ── 音频页面 ──
    /// 音频页面标题
    pub audio_title: &'static str,
    /// 合成器
    pub synthesizer: &'static str,
    /// 音色库
    pub soundfont: &'static str,
    /// 音色库选择占位符
    pub soundfont_placeholder: &'static str,
    /// 缓冲区（延迟）
    pub buffer_latency: &'static str,
    /// 每键最大同音数
    pub max_voices: &'static str,
    /// 每键最大同音数提示
    pub max_voices_hint: &'static str,
    /// 全局最大复音数（硬上限/量程）
    pub global_voice_limit: &'static str,
    /// 全局最大复音数：自动
    pub global_voice_limit_auto: &'static str,
    /// 全局最大复音数提示
    pub global_voice_limit_hint: &'static str,
    /// 复音软目标比例
    pub voice_target_ratio: &'static str,
    /// 复音软目标比例提示
    pub voice_target_ratio_hint: &'static str,
    /// 过载保命闸
    pub soft_nps_gate: &'static str,
    /// 过载保命闸提示
    pub soft_nps_gate_hint: &'static str,
    /// 力度过滤阈值
    pub velocity_filter: &'static str,
    /// 力度过滤阈值提示
    pub velocity_filter_hint: &'static str,
    /// MIDI 输入设备
    pub midi_input_device: &'static str,
    /// 无可用设备
    pub no_device: &'static str,
    /// 选择设备占位符
    pub select_device_placeholder: &'static str,
    /// KDMAPI 模式提示
    pub kdmapi_hint: &'static str,
    /// System 模式提示
    pub system_hint: &'static str,
    /// WinMM 输出设备（播表）标签
    pub winmm_output_device: &'static str,
    /// WinMM 输出设备未检测到提示
    pub winmm_no_device: &'static str,
    /// 刷新按钮标签
    pub refresh: &'static str,
    /// XSynth 模式提示
    pub xsynth_hint: &'static str,
    /// LGS (GPU) 模式提示
    pub lgs_hint: &'static str,
    /// 内置合成器引擎子下拉标签
    pub builtin_engine: &'static str,
    /// LGS (GPU) 缓冲区大小标签
    pub lgs_buffer: &'static str,
    /// LGS (GPU) 缓冲区大小提示
    pub lgs_buffer_hint: &'static str,
    /// 音频播放输出设备标签
    pub audio_output_device: &'static str,
    /// 音频播放输出设备默认项（系统默认输出设备）
    pub audio_output_default: &'static str,
    /// 音频播放输出设备未检测到提示
    pub audio_output_no_device: &'static str,
    /// 音频播放输出设备提示
    pub audio_output_hint: &'static str,

    // ── 界面页面 ──
    /// 界面页面标题
    pub ui_title: &'static str,
    /// 主题
    pub theme: &'static str,
    /// 程序字体
    pub program_font: &'static str,
    /// 字体路径占位符
    pub font_path_placeholder: &'static str,
    /// 字体提示
    pub font_hint: &'static str,
    /// 使用原生标题栏
    pub native_titlebar: &'static str,
    /// 原生标题栏提示
    pub native_titlebar_hint: &'static str,
    /// 启用 HiDPI 图标渲染
    pub hidpi_icon: &'static str,
    /// HiDPI 图标渲染提示
    pub hidpi_icon_hint: &'static str,
    /// 自动滚动设置
    pub auto_scroll: &'static str,
    /// 自动滚动模式1 - 固定位置
    pub auto_scroll_fixed: &'static str,
    /// 自动滚动模式2 - 翻页触发位置
    pub auto_scroll_trigger: &'static str,
    /// 自动滚动模式2 - 翻页后位置
    pub auto_scroll_return: &'static str,
    /// 自动滚动提示
    pub auto_scroll_hint: &'static str,
    /// 交互
    pub interaction: &'static str,
    /// 框选框模式
    pub selection_box_mode: &'static str,
    /// 框选框模式提示
    pub selection_box_hint: &'static str,
    /// 钢琴卷帘
    pub piano_roll: &'static str,
    /// 启用 256 键扩展钢琴卷帘
    pub enable_256key: &'static str,
    /// 256 键扩展提示
    pub enable_256key_hint: &'static str,
    /// 像素
    pub pixel: &'static str,
    /// 从左边缘算起
    pub from_left: &'static str,
    /// 从右边缘算起
    pub from_right: &'static str,

    // ── 快捷键页面 ──
    /// 快捷键页面标题
    pub shortcuts_title: &'static str,
    /// 快捷键设置占位符
    pub shortcuts_placeholder: &'static str,

    // ── 关于页面 ──
    /// 关于页面标题
    pub about_title: &'static str,
    /// 应用名称
    pub app_name: &'static str,

    // ── 调色板页面 ──
    /// 调色板页面标题
    pub palette_title: &'static str,
    /// 选择调色板
    pub palette_select: &'static str,
    /// 调色板提示
    pub palette_hint: &'static str,
    /// 颜色信息
    pub palette_colors_info: &'static str,
    /// 无预览
    pub palette_no_preview: &'static str,
    /// 调色板已锁定
    pub palette_locked: &'static str,
    /// 版本
    pub version: &'static str,
    /// 应用描述
    pub app_description: &'static str,
    /// 高对比度主题显示名称
    pub high_contrast: &'static str,

    // ── 编辑页面 ──
    /// 编辑页面标题
    pub editing_title: &'static str,
    /// 操作历史分区
    pub editing_history_section: &'static str,
    /// 操作日志总条数上限
    pub editing_history_total_limit: &'static str,
    /// 操作日志总条数上限提示
    pub editing_history_total_limit_hint: &'static str,
    /// 单条日志条目上限
    pub editing_history_entry_limit: &'static str,
    /// 单条日志条目上限提示
    pub editing_history_entry_limit_hint: &'static str,
    /// 合并窗口
    pub editing_merge_window: &'static str,
    /// 合并窗口提示
    pub editing_merge_window_hint: &'static str,
    /// 编辑拦截分区
    pub editing_intercept_section: &'static str,
    /// 拦截时显示通知
    pub editing_intercept_notification: &'static str,
    /// 拦截通知提示
    pub editing_intercept_notification_hint: &'static str,
    /// Tempo 面板 BPM 上限
    pub editing_tempo_max_bpm: &'static str,
    /// BPM 上限提示
    pub editing_tempo_max_bpm_hint: &'static str,
    /// 自定义 BPM 上限弹窗
    pub editing_tempo_custom_option: &'static str,
    /// 自定义 BPM 上限标题
    pub editing_tempo_custom_title: &'static str,
    /// 自定义 BPM 上限占位符
    pub editing_tempo_custom_placeholder: &'static str,
    /// 自动化曲线连线粗细
    pub ui_automation_line_thickness: &'static str,
    /// 自动化曲线连线粗细提示
    pub ui_automation_line_thickness_hint: &'static str,
    /// 日志存储份数
    pub log_retention_section: &'static str,
    /// 日志文件保留份数
    pub log_retention_count: &'static str,
    /// 日志文件保留份数提示
    pub log_retention_count_hint: &'static str,
    /// 底边栏监控数据刷新间隔
    pub ui_monitor_refresh_interval: &'static str,
    /// 监控数据刷新间隔提示
    pub ui_monitor_refresh_interval_hint: &'static str,

    // ── 兼容性页面 ──
    /// 兼容性菜单项
    pub compatibility: &'static str,
    /// 兼容性页面标题
    pub compatibility_title: &'static str,
    /// 手动检查按钮
    pub compat_check_button: &'static str,
    /// 检查中提示
    pub compat_checking: &'static str,
    /// 尚未检查提示
    pub compat_never_checked: &'static str,
    /// 检测通过
    pub compat_passed: &'static str,
    /// 检测失败
    pub compat_failed: &'static str,
    /// 复制诊断信息按钮
    pub compat_copy_diagnostics: &'static str,
    /// 已复制提示
    pub compat_copied: &'static str,
    /// 每次启动检查开关
    pub compat_check_on_startup: &'static str,
    /// 每次启动检查提示
    pub compat_check_on_startup_hint: &'static str,
    /// 启动警告开关
    pub compat_show_warning: &'static str,
    /// 启动警告提示
    pub compat_show_warning_hint: &'static str,
}

/// 获取设置面板翻译
pub fn get(lang: Language) -> &'static SettingsTranslations {
    match lang {
        Language::ZhCn => &ZHCN_SETTINGS,
        Language::EnUs => &ENUS_SETTINGS,
    }
}

#[cfg(test)]
mod tests;
