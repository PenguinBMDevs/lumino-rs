//! 翻译数据/字典 — 主界面翻译的静态数据（MainTranslations 结构体 + 中英文翻译表）

/// 主界面翻译
#[derive(Debug, Clone)]
pub struct MainTranslations {
    /// 快退
    pub skip_backward: &'static str,
    /// 暂停
    pub pause: &'static str,
    /// 播放
    pub play: &'static str,
    /// 快进
    pub skip_forward: &'static str,
    /// 循环播放：开
    pub loop_on: &'static str,
    /// 循环播放：关
    pub loop_off: &'static str,
    /// 切换循环播放提示
    pub toggle_loop_tooltip: &'static str,
    /// 开始录制
    pub record_start: &'static str,
    /// 停止录制
    pub record_stop: &'static str,
    /// 选择工具
    pub tool_pointer: &'static str,
    /// Y 向框选工具
    pub tool_pointer_y_select: &'static str,
    /// 铅笔工具
    pub tool_pencil: &'static str,
    /// 橡皮擦
    pub tool_eraser: &'static str,
    /// 曲线工具
    pub tool_curve: &'static str,
    /// 颜料桶（填充封闭区域）
    pub tool_fill: &'static str,
    /// 量化
    pub tool_quantize: &'static str,
    /// 变速
    pub tool_speed: &'static str,
    /// 垂直翻转
    pub tool_flip_vertical: &'static str,
    /// 水平翻转
    pub tool_flip_horizontal: &'static str,
    /// 分割（Split）
    pub tool_split: &'static str,
    /// 合并（Glue）
    pub tool_glue: &'static str,
    /// 连奏（Tie）
    pub tool_tie: &'static str,
    /// 移调 -1
    pub tool_transpose_down: &'static str,
    /// 移调 +1
    pub tool_transpose_up: &'static str,
    /// 低八度
    pub tool_transpose_down_octave: &'static str,
    /// 高八度
    pub tool_transpose_up_octave: &'static str,
    /// 精度标签
    pub precision_label: &'static str,
    /// 精度选择占位符
    pub precision_placeholder: &'static str,
    /// 自动滚动：固定
    pub auto_scroll_fixed: &'static str,
    /// 自动滚动：滚动
    pub auto_scroll_scrolling: &'static str,
    /// 自动滚动：关闭
    pub auto_scroll_off: &'static str,
    /// 切换自动滚动模式提示
    pub auto_scroll_tooltip: &'static str,
    /// 多人协作标签
    pub collaboration_label: &'static str,
    /// 打开协作面板提示
    pub collaboration_tooltip: &'static str,
    /// 更多工具
    pub toolbar_more: &'static str,
    /// 图片转 MIDI
    pub tool_image_to_midi: &'static str,
    /// 撤销
    pub undo: &'static str,
    /// 重做
    pub redo: &'static str,
    /// 文件菜单
    pub menu_file: &'static str,
    /// 编辑菜单
    pub menu_edit: &'static str,
    /// 视图菜单
    pub menu_view: &'static str,
    /// 帮助菜单
    pub menu_help: &'static str,
    /// 新建
    pub file_new: &'static str,
    /// 打开
    pub file_open: &'static str,
    /// 保存
    pub file_save: &'static str,
    /// 关闭
    pub file_close: &'static str,
    /// 导入文件
    pub file_import: &'static str,
    /// 从云导入
    pub file_import_from_cloud: &'static str,
    /// 保存到云
    pub file_save_to_cloud: &'static str,
    /// 导出工程
    pub file_export_project: &'static str,
    /// 导出为单文件
    pub file_export_archive: &'static str,
    /// 导出为文件夹
    pub file_export_folder: &'static str,
    /// 导出为素材
    pub file_export_material: &'static str,
    /// 导出音频
    pub file_export_audio: &'static str,
    /// 工程设置
    pub file_project_settings: &'static str,
    /// 设置
    pub file_settings: &'static str,
    /// 退出
    pub file_exit: &'static str,
    /// 编辑：撤销
    pub edit_undo: &'static str,
    /// 编辑：重做
    pub edit_redo: &'static str,
    /// 编辑：剪切
    pub edit_cut: &'static str,
    /// 编辑：复制
    pub edit_copy: &'static str,
    /// 编辑：粘贴
    pub edit_paste: &'static str,
    /// 编辑：全选
    pub edit_select_all: &'static str,
    /// 编辑：查找
    pub edit_find: &'static str,
    /// 视图：放大
    pub view_zoom_in: &'static str,
    /// 视图：缩小
    pub view_zoom_out: &'static str,
    /// 视图：重置缩放
    pub view_zoom_reset: &'static str,
    /// 帮助：关于
    pub help_about: &'static str,
    /// 模式：编辑器
    pub mode_editor: &'static str,
    /// 模式：瀑布流
    pub mode_waterfall: &'static str,
    /// 切换到编辑器模式
    pub mode_switch_to_editor: &'static str,
    /// 切换到瀑布流模式
    pub mode_switch_to_waterfall: &'static str,
    /// 状态：就绪
    pub status_ready: &'static str,
    /// 保存成功后的底边栏提示
    pub status_file_saved: &'static str,
    /// 保存失败后的底边栏提示前缀
    pub status_save_failed: &'static str,
    /// 侧边栏：音轨列表
    pub sidebar_file: &'static str,
    /// 侧边栏：音轨总览
    pub sidebar_arrangement: &'static str,
    /// 侧边栏：自动化面板
    pub sidebar_automation: &'static str,
    /// 侧边栏：音轨列表
    pub sidebar_track_list: &'static str,
    /// 侧边栏：添加音轨
    pub sidebar_add_track: &'static str,
    /// 分音符
    pub precision_note_label: &'static str,
    /// 除以
    pub precision_divide_by: &'static str,
    /// 确定
    pub precision_ok: &'static str,
    /// 取消
    pub precision_cancel: &'static str,

    // ── 力度/速度编辑面板 ──
    /// 力度标签
    pub velocity_panel_velocity: &'static str,
    /// 速度标签
    pub velocity_panel_tempo: &'static str,
    /// 力度信息说明
    pub velocity_panel_velocity_info: &'static str,
    /// 速度信息说明
    pub velocity_panel_tempo_info: &'static str,

    // ── 工程设置对话框 ──
    /// 工程设置标题
    pub project_title: &'static str,
    /// 项目名称标签
    pub project_name_label: &'static str,
    /// 项目名称占位符
    pub project_name_placeholder: &'static str,
    /// BPM 速度标签
    pub project_bpm_label: &'static str,
    /// 版权信息标签
    pub project_copyright_label: &'static str,
    /// 版权信息占位符
    pub project_copyright_placeholder: &'static str,
    /// 作者标签
    pub project_author_label: &'static str,
    /// 作者占位符
    pub project_author_placeholder: &'static str,
    /// 创建日期标签
    pub project_created_label: &'static str,
    /// 未知
    pub project_unknown: &'static str,
    /// 累计创作时间标签
    pub project_editing_time_label: &'static str,
    /// 工程设置确定按钮
    pub project_ok: &'static str,
    /// 工程设置取消按钮
    pub project_cancel: &'static str,

    // ── 素材库面板 ──
    /// 素材库标题
    pub material_library: &'static str,
    /// 添加素材
    pub material_add: &'static str,
    /// 从云导入素材
    pub material_download_web: &'static str,
    /// 从本地选取素材
    pub material_import_local: &'static str,
    /// 内置素材分区
    pub material_section_builtin: &'static str,
    /// 本地素材分区
    pub material_section_user: &'static str,
    /// 素材描述悬浮窗标头：名称
    pub material_name_label: &'static str,
    /// 素材描述悬浮窗标头：作者
    pub material_author_label: &'static str,
    /// 素材描述悬浮窗标头：位置（磁盘路径）
    pub material_location_label: &'static str,
    /// 素材描述悬浮窗标头：轨道数
    pub material_track_label: &'static str,
    /// 素材描述悬浮窗标头：来源
    pub material_source_label: &'static str,
    /// 素材无效
    pub material_invalid: &'static str,
    /// 素材已导入
    pub material_import_ok: &'static str,
    /// 素材导入失败
    pub material_import_failed: &'static str,
    /// 删除素材确认对话框标题
    pub material_delete_title: &'static str,
    /// 删除素材危险提示（不可恢复）
    pub material_delete_warning: &'static str,
    /// 删除素材确认按钮
    pub material_delete: &'static str,
    /// 删除素材取消按钮
    pub material_delete_cancel: &'static str,
}

pub(crate) static ZHCN_MAIN: MainTranslations = MainTranslations {
    skip_backward: "快退",
    pause: "暂停",
    play: "播放",
    skip_forward: "快进",
    loop_on: "循环播放: 开",
    loop_off: "循环播放: 关",
    toggle_loop_tooltip: "切换循环播放",
    record_start: "开始录制",
    record_stop: "停止录制",
    tool_pointer: "选择工具",
    tool_pointer_y_select: "Y向框选工具",
    tool_pencil: "铅笔工具",
    tool_eraser: "橡皮擦",
    tool_curve: "曲线工具",
    tool_fill: "颜料桶（填充封闭区域）",
    tool_quantize: "量化",
    tool_speed: "变速",
    tool_flip_vertical: "垂直翻转",
    tool_flip_horizontal: "水平翻转",
    tool_split: "分割(Split)",
    tool_glue: "合并(Glue)",
    tool_tie: "连奏(Tie)",
    tool_transpose_down: "移调 -1",
    tool_transpose_up: "移调 +1",
    tool_transpose_down_octave: "低八度",
    tool_transpose_up_octave: "高八度",
    precision_label: "精度:",
    precision_placeholder: "选择精度",
    auto_scroll_fixed: "自动滚动: 固定",
    auto_scroll_scrolling: "自动滚动: 滚动",
    auto_scroll_off: "自动滚动: 关闭",
    auto_scroll_tooltip: "切换自动滚动模式",
    collaboration_label: "多人协作",
    collaboration_tooltip: "打开协作面板",
    toolbar_more: "更多工具",
    tool_image_to_midi: "图片转MIDI",
    undo: "撤销",
    redo: "重做",
    menu_file: "文件",
    menu_edit: "编辑",
    menu_view: "视图",
    menu_help: "帮助",
    file_new: "新建",
    file_open: "打开",
    file_save: "保存",
    file_close: "关闭",
    file_import: "导入文件",
    file_import_from_cloud: "从云导入",
    file_save_to_cloud: "保存到云",
    file_export_project: "导出工程",
    file_export_archive: "导出为单文件",
    file_export_folder: "导出为文件夹",
    file_export_material: "导出为素材",
    file_export_audio: "导出音频",
    file_project_settings: "工程设置",
    file_settings: "设置",
    file_exit: "退出",
    edit_undo: "撤销",
    edit_redo: "重做",
    edit_cut: "剪切",
    edit_copy: "复制",
    edit_paste: "粘贴",
    edit_select_all: "全选",
    edit_find: "查找",
    view_zoom_in: "放大",
    view_zoom_out: "缩小",
    view_zoom_reset: "重置缩放",
    help_about: "关于",
    mode_editor: "编辑器",
    mode_waterfall: "瀑布流",
    mode_switch_to_editor: "切换到编辑器模式",
    mode_switch_to_waterfall: "切换到瀑布流模式",
    status_ready: "就绪",
    status_file_saved: "文件已经保存",
    status_save_failed: "保存失败",
    sidebar_file: "音轨列表",
    sidebar_arrangement: "音轨总览",
    sidebar_automation: "自动化面板",
    sidebar_track_list: "音轨列表",
    sidebar_add_track: "添加音轨",
    precision_note_label: "分音符",
    precision_divide_by: "除以",
    precision_ok: "确定",
    precision_cancel: "取消",

    // 力度/速度编辑面板
    velocity_panel_velocity: "力度",
    velocity_panel_tempo: "速度",
    velocity_panel_velocity_info: "力度 0-127",
    velocity_panel_tempo_info: "速度 BPM",

    project_title: "工程信息设置",
    project_name_label: "项目名称",
    project_name_placeholder: "输入项目名称（留空显示为'无标题'）",
    project_bpm_label: "BPM 速度",
    project_copyright_label: "版权信息",
    project_copyright_placeholder: "输入版权信息（可选）",
    project_author_label: "作者",
    project_author_placeholder: "输入作者（可选）",
    project_created_label: "创建日期",
    project_unknown: "未知",
    project_editing_time_label: "累计创作时间",
    project_ok: "确定",
    project_cancel: "取消",

    material_library: "素材库",
    material_add: "添加素材",
    material_download_web: "从云导入",
    material_import_local: "从本地选取",
    material_section_builtin: "内置素材",
    material_section_user: "本地素材",
    material_name_label: "名称：",
    material_author_label: "作者：",
    material_location_label: "位置：",
    material_track_label: "轨道数：",
    material_source_label: "来源：",
    material_invalid: "素材无效",
    material_import_ok: "素材已导入",
    material_import_failed: "素材导入失败",
    material_delete_title: "删除素材",
    material_delete_warning: "删除后无法恢复，确定要删除吗？",
    material_delete: "删除",
    material_delete_cancel: "取消",
};

pub(crate) static ENUS_MAIN: MainTranslations = MainTranslations {
    skip_backward: "Skip Backward",
    pause: "Pause",
    play: "Play",
    skip_forward: "Skip Forward",
    loop_on: "Loop: On",
    loop_off: "Loop: Off",
    toggle_loop_tooltip: "Toggle Loop Playback",
    record_start: "Start Recording",
    record_stop: "Stop Recording",
    tool_pointer: "Select Tool",
    tool_pointer_y_select: "Y-Axis Box Select",
    tool_pencil: "Pencil Tool",
    tool_eraser: "Eraser",
    tool_curve: "Curve Tool",
    tool_fill: "Paint Bucket (fill enclosed regions)",
    tool_quantize: "Quantize",
    tool_speed: "Speed Change",
    tool_flip_vertical: "Flip Vertical",
    tool_flip_horizontal: "Flip Horizontal",
    tool_split: "Split",
    tool_glue: "Glue",
    tool_tie: "Tie (Legato)",
    tool_transpose_down: "Transpose -1",
    tool_transpose_up: "Transpose +1",
    tool_transpose_down_octave: "Lower Octave",
    tool_transpose_up_octave: "Raise Octave",
    precision_label: "Precision:",
    precision_placeholder: "Select precision",
    auto_scroll_fixed: "Auto-Scroll: Fixed",
    auto_scroll_scrolling: "Auto-Scroll: Scroll",
    auto_scroll_off: "Auto-Scroll: Off",
    auto_scroll_tooltip: "Toggle Auto-Scroll Mode",
    collaboration_label: "Collaborate",
    collaboration_tooltip: "Open Collaboration Panel",
    toolbar_more: "More Tools",
    tool_image_to_midi: "Image to MIDI",
    undo: "Undo",
    redo: "Redo",
    menu_file: "File",
    menu_edit: "Edit",
    menu_view: "View",
    menu_help: "Help",
    file_new: "New",
    file_open: "Open",
    file_save: "Save",
    file_close: "Close",
    file_import: "Import Files",
    file_import_from_cloud: "Import from Cloud",
    file_save_to_cloud: "Save to Cloud",
    file_export_project: "Export Project",
    file_export_archive: "Export as Single File",
    file_export_folder: "Export as Folder",
    file_export_material: "Export as Material",
    file_export_audio: "Export Audio",
    file_project_settings: "Project Settings",
    file_settings: "Settings",
    file_exit: "Exit",
    edit_undo: "Undo",
    edit_redo: "Redo",
    edit_cut: "Cut",
    edit_copy: "Copy",
    edit_paste: "Paste",
    edit_select_all: "Select All",
    edit_find: "Find",
    view_zoom_in: "Zoom In",
    view_zoom_out: "Zoom Out",
    view_zoom_reset: "Reset Zoom",
    help_about: "About",
    mode_editor: "Editor",
    mode_waterfall: "Waterfall",
    mode_switch_to_editor: "Switch to Editor Mode",
    mode_switch_to_waterfall: "Switch to Waterfall Mode",
    status_ready: "Ready",
    status_file_saved: "File saved",
    status_save_failed: "Save failed",
    sidebar_file: "Track List",
    sidebar_arrangement: "Track Overview",
    sidebar_automation: "Automation",
    sidebar_track_list: "Track List",
    sidebar_add_track: "Add Track",
    precision_note_label: "note",
    precision_divide_by: "divide by",
    precision_ok: "OK",
    precision_cancel: "Cancel",

    // Velocity/Tempo panel
    velocity_panel_velocity: "Velocity",
    velocity_panel_tempo: "Tempo",
    velocity_panel_velocity_info: "Velocity 0-127",
    velocity_panel_tempo_info: "Tempo BPM",

    project_title: "Project Settings",
    project_name_label: "Project Name",
    project_name_placeholder: "Enter project name (blank = 'Untitled')",
    project_bpm_label: "BPM Tempo",
    project_copyright_label: "Copyright",
    project_copyright_placeholder: "Enter copyright info (optional)",
    project_author_label: "Author",
    project_author_placeholder: "Enter author (optional)",
    project_created_label: "Created",
    project_unknown: "Unknown",
    project_editing_time_label: "Total Editing Time",
    project_ok: "OK",
    project_cancel: "Cancel",

    material_library: "Material Library",
    material_add: "Add Material",
    material_download_web: "Import from Cloud",
    material_import_local: "Import from Local",
    material_section_builtin: "Built-in",
    material_section_user: "Local",
    material_name_label: "Name: ",
    material_author_label: "Author: ",
    material_location_label: "Location: ",
    material_track_label: "Tracks: ",
    material_source_label: "Source: ",
    material_invalid: "Invalid material",
    material_import_ok: "Material imported",
    material_import_failed: "Failed to import material",
    material_delete_title: "Delete Material",
    material_delete_warning: "This cannot be undone. Delete anyway?",
    material_delete: "Delete",
    material_delete_cancel: "Cancel",
};
