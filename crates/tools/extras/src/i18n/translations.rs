//! 翻译数据/字典 — 主界面翻译的静态数据（MainTranslations 结构体 + 中英文翻译表）

#[path = "translations/enus.rs"]
mod enus;
#[path = "translations/zhcn.rs"]
mod zhcn;

pub(crate) use enus::ENUS_MAIN;
pub(crate) use zhcn::ZHCN_MAIN;

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
    /// 音符绘制工具集触发按钮（工具栏小三角）
    pub tool_panel_tooltip: &'static str,
    /// 描边设置
    pub tool_stroke: &'static str,
    /// 画刷工具
    pub tool_brush: &'static str,
    /// 形状工具
    pub tool_shape: &'static str,
    /// 文字输入
    pub tool_text: &'static str,
    /// 曲线工具说明（绘制工具面板底部描述条）
    pub tool_curve_desc: &'static str,
    /// 颜料桶说明（绘制工具面板底部描述条）
    pub tool_fill_desc: &'static str,
    /// 画刷工具说明（绘制工具面板底部描述条）
    pub tool_brush_desc: &'static str,
    /// 形状工具说明（绘制工具面板底部描述条）
    pub tool_shape_desc: &'static str,
    /// 文字输入说明（绘制工具面板底部描述条）
    pub tool_text_desc: &'static str,
    /// 橡皮擦说明（绘制工具面板底部描述条）
    pub tool_eraser_desc: &'static str,
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
    /// 导出为 MIDI 文件
    pub file_export_midi: &'static str,
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
    /// 音符总量（工程设置对话框 / 下边栏；UI-015）
    pub note_count_label: &'static str,
    /// 保存成功后的底边栏提示
    pub status_file_saved: &'static str,
    /// 保存失败后的底边栏提示前缀
    pub status_save_failed: &'static str,
    /// 导出 MIDI 成功后的底边栏提示
    pub status_midi_exported: &'static str,
    /// 没有可导出的 MIDI 内容时的底边栏提示
    pub status_no_midi_content: &'static str,
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
    /// 钢琴瀑布流预览标题
    pub piano_waterfall: &'static str,
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
