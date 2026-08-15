//! 右侧栏动作

use crate::context_menu::MaterialContextMenuItem;

/// 右侧栏动作
#[derive(Debug, Clone)]
pub enum RightSidebarAction {
    /// 点击图片转 MIDI 按钮（展开/收起面板并亮灯）
    ImageToMidiClicked,
    /// 面板内点击"选择图片文件"按钮（弹出文件对话框）
    SelectImageFile,
    /// 面板内点击"转换为 MIDI"按钮（调用 i2m-rs 转换并进入放置模式）
    ConvertClicked,
    /// 放置模式悬浮按钮：√ 确认（写入 document）
    PlacementConfirm,
    /// 放置模式悬浮按钮：× 取消（还原显示区域）
    PlacementCancel,
    /// 更新 i2m 数字配置项（文本仅接受数字；空串仅保留输入缓冲）
    I2mConfigTextChanged { field: I2mConfigField, text: String },
    /// 切换调色板算法（索引指向 `PALETTE_ALGORITHMS`）
    I2mPaletteChanged(usize),
    /// 开始拖拽调整面板宽度
    ResizeDragStarted,
    /// 拖拽中调整面板宽度
    ResizeDragged,
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
    /// 点击素材库按钮（展开/收起面板并亮灯）
    MaterialLibraryClicked,
    /// 面板内点击"添加素材"按钮（展开/收起下拉菜单）
    MaterialAddClicked,
    /// 下拉菜单："从 web 下载"（占位实现，仅记录日志）
    MaterialDownloadFromWeb,
    /// 下拉菜单："从本地选取"（文件对话框导入 .lmmaterial 并复制到配置目录）
    MaterialImportFromLocal,
    /// 关闭素材添加下拉菜单
    MaterialAddMenuClosed,
    /// 素材项按下（开始拖出：加载素材到内存并显示跟随预览）
    MaterialDragStarted(usize),
    /// 素材项右键（打开素材右键菜单，index 为素材列表索引）
    MaterialContextMenuOpened(usize),
    /// 关闭素材右键菜单
    MaterialContextMenuClosed,
    /// 点击素材右键菜单项
    MaterialContextMenuItemClicked(usize, MaterialContextMenuItem),
    /// 素材重命名输入框内容变更
    MaterialRenameInputChanged(String),
    /// 确认素材重命名（同步文件与 metadata 名称）
    MaterialRenameConfirmed,
    /// 取消素材重命名
    MaterialRenameCancelled,
    /// 确认删除素材（删除本地文件并重新扫描）
    MaterialDeleteConfirmed(usize),
    /// 取消删除素材确认
    MaterialDeleteCancelled,
}

/// i2m 转换配置中的数字编辑字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2mConfigField {
    /// 最低 MIDI key（0-127）
    StartKey,
    /// 最高 MIDI key（0-127）
    EndKey,
    /// 目标高度（像素，0=保持宽高比）
    TargetHeight,
    /// 每像素行对应的 MIDI tick（>0）
    TicksPerPixel,
    /// 调色板颜色数（=生成音轨数）
    ColorCount,
}
