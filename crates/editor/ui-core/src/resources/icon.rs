//! 图标枚举（仅数据；SVG 渲染已迁 \lumino-ui-egui::icons\）。
//!
//! 旧版 视图层删除后本模块只保留 \Icon\ 变体与 HiDPI 兼容开关：
//! 绘制由 egui 的 \IconCache\ 负责，不再包含 SVG widget / 光栅化代码。

use std::sync::atomic::{AtomicBool, Ordering};

/// 当前 HiDPI 图标渲染状态（兼容旧设置项；egui 矢量直渲无需 2x 位图）
static HIDPI_ENABLED: AtomicBool = AtomicBool::new(true);

pub use Icon::*;
// ─── 图标定义宏：一处定义 → 三处生成（枚举 + 全部枚举数组 + bytes 匹配） ───
macro_rules! define_icons {
    ($(($name:ident, $path:expr)),* $(,)?) => {
        /// 图标变体枚举（由 define_icons! 宏生成）
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $(/// 图标变体 `Icon::$name`
            $name,)*
        }

        /// 全部图标枚举（与宏定义同源，供测试/遍历使用）
        pub const ALL_ICONS: &[Icon] = &[$($name,)*];

        /// 图标 SVG 原始字节（图标→路径的单一来源；egui 图标缓存可复用）
        #[allow(dead_code)]
        pub(crate) fn bytes(icon: Icon) -> &'static [u8] {
            match icon {
                $(Icon::$name => include_bytes!($path),)*
            }
        }
    };
}

define_icons! {
    (AngleRight, "../../../../../resources/icons/regular/submenu-expand-indicator.svg"),
    (FolderTree, "../../../../../resources/icons/regular/ui-layout.svg"),
    (Arrangement, "../../../../../resources/icons/regular/arrangement-mode.svg"),
    (Gear, "../../../../../resources/icons/regular/settings-general.svg"),
    (WaveForm, "../../../../../resources/icons/regular/audio-automation.svg"),
    (Lumino, "../../../../../resources/icons/brands/lumino-brand.svg"),
    (LogoInApp, "../../../../../resources/icons/brands/app-logo.svg"),
    (WindowMin, "../../../../../resources/icons/window/min.svg"),
    (WindowMax, "../../../../../resources/icons/window/max.svg"),
    (WindowUnMax, "../../../../../resources/icons/window/unmax.svg"),
    (WindowClose, "../../../../../resources/icons/window/close.svg"),
    (Clock, "../../../../../resources/icons/sidebar/conductor-track.svg"),
    (Eye, "../../../../../resources/icons/sidebar/onion-skin.svg"),
    (EyeSlash, "../../../../../resources/icons/sidebar/onion-skin-disabled.svg"),
    (Plus, "../../../../../resources/icons/sidebar/add-track.svg"),
    (Download, "../../../../../resources/icons/sidebar/export-group.svg"),
    (PlayCircle, "../../../../../resources/icons/sidebar/waterfall-record.svg"),
    (EllipsisVertical, "../../../../../resources/icons/sidebar/toolbar-overflow-trigger.svg"),
    // 卷帘面板左侧栏底部按钮（横向 / 纵向三条杠）
    (RollBarHorizontal, "../../../../../resources/icons/sidebar/roll-bar-horizontal.svg"),
    (RollBarVertical, "../../../../../resources/icons/sidebar/roll-bar-vertical.svg"),
    (Users, "../../../../../resources/icons/toolbar/collaboration.svg"),
    // 工具栏图标
    (Play, "../../../../../resources/icons/toolbar/playback-start.svg"),
    (Pause, "../../../../../resources/icons/toolbar/playback-pause.svg"),
    (SkipBackward, "../../../../../resources/icons/toolbar/playback-jump-start.svg"),
    (SkipForward, "../../../../../resources/icons/toolbar/playback-jump-end.svg"),
    (Undo, "../../../../../resources/icons/toolbar/history-undo.svg"),
    (Redo, "../../../../../resources/icons/toolbar/history-redo.svg"),
    (MousePointer, "../../../../../resources/icons/toolbar/select-tool.svg"),
    (MousePointerYSelect, "../../../../../resources/icons/toolbar/y-axis-select-tool.svg"),
    (Pencil, "../../../../../resources/icons/toolbar/note-draw-tool.svg"),
    (Eraser, "../../../../../resources/icons/toolbar/eraser-tool.svg"),
    (Curve, "../../../../../resources/icons/toolbar/curve-tool.svg"),
    (PaintBucket, "../../../../../resources/icons/toolbar/paint-bucket.svg"),
    // 「音符画工具箱」入口图标（工具栏开关按钮，固定不随激活工具切换）
    (DrawToolbox, "../../../../../resources/icons/toolbar/draw-toolbox.svg"),
    // 洋葱皮音轨的「洋葱」图形（工具栏全局显示开关的主图标）
    (OnionSkin, "../../../../../resources/icons/toolbar/onion-skin.svg"),
    // 工具栏**分裂式开关**右侧的「下拉三角」：视觉上标出"此处可展开选项"。
    //
    // 原名 `ToolPanelCaret`（当年挂在颜料桶右侧触发旧「绘制工具选择面板」）：该面板
    // 已迁到音符画悬浮工具条，本资源自那以后无调用点。现由洋葱皮全局开关重新启用，
    // 名字收敛为与语义无关的 `CaretDown`——后续任何分裂按钮都可复用它。
    (CaretDown, "../../../../../resources/icons/toolbar/caret-down.svg"),
    // 悬浮条**右端「粗细」按钮**的图标：上下两条粗细不同的横线。
    // 文件沿用它原本的名字 `stroke-settings.svg`（SVG 术语里 stroke-width 就是线宽/粗细）；
    // 该图标此前从未被任何视图渲染过（旧「绘制工具下拉」的描边设置条目胎死腹中），本轮激活。
    (Thickness, "../../../../../resources/icons/toolbar/stroke-settings.svg"),
    // 绘制工具选择面板条目图标
    (BrushTool, "../../../../../resources/icons/toolbar/brush-tool.svg"),
    (ShapeTool, "../../../../../resources/icons/toolbar/shape-tool.svg"),
    (TextInput, "../../../../../resources/icons/toolbar/text-input.svg"),
    // 形状工具下拉中的图形类型图标（ctrl+点击形状工具弹出）：矩形/圆形/三角形
    (ShapeRectangle, "../../../../../resources/icons/toolbar/shape-rectangle.svg"),
    (ShapeCircle, "../../../../../resources/icons/toolbar/shape-circle.svg"),
    (ShapeTriangle, "../../../../../resources/icons/toolbar/shape-triangle.svg"),
    // 画刷下拉/绘制行为对话框：圆形 +/- 按钮（SVG 绘制）
    (PlusCircle, "../../../../../resources/icons/toolbar/plus-circle.svg"),
    (MinusCircle, "../../../../../resources/icons/toolbar/minus-circle.svg"),
    (Quantize, "../../../../../resources/icons/toolbar/note-quantize.svg"),
    (Speed, "../../../../../resources/icons/toolbar/playback-speed.svg"),
    // 音符翻转图标
    (FlipVertical, "../../../../../resources/icons/toolbar/note-flip-vertical.svg"),
    (FlipHorizontal, "../../../../../resources/icons/toolbar/note-flip-horizontal.svg"),
    // 自动滚动图标
    (ArrowsLeftRight, "../../../../../resources/icons/toolbar/loop-range-active.svg"),
    (Scroll, "../../../../../resources/icons/toolbar/autoscroll-scrolling.svg"),
    (Ban, "../../../../../resources/icons/toolbar/loop-range-disabled.svg"),
    // 移调/分割/合并 图标
    (TransposeUp, "../../../../../resources/icons/toolbar/note-transpose-up.svg"),
    (TransposeDown, "../../../../../resources/icons/toolbar/note-transpose-down.svg"),
    (Split, "../../../../../resources/icons/toolbar/note-split.svg"),
    (Glue, "../../../../../resources/icons/toolbar/note-glue.svg"),
    // 连奏/同音连接
    (Tie, "../../../../../resources/icons/toolbar/note-tie.svg"),
    // 图片转 MIDI 占位图标
    (ImageToMidi, "../../../../../resources/icons/toolbar/image-to-midi-converter.svg"),
    // 素材库图标（右侧栏）
    (MaterialLibrary, "../../../../../resources/icons/toolbar/material-library.svg"),
    // 钢琴瀑布流预览图标（右侧栏）
    (PianoWaterfall, "../../../../../resources/icons/sidebar/piano-waterfall.svg"),
    // 标题栏图标
    (PencilOutline, "../../../../../resources/icons/titlebar/editor-mode.svg"),
    (Keys, "../../../../../resources/icons/titlebar/piano-roll.svg"),
    (VideoCamera, "../../../../../resources/icons/sidebar/video-export.svg"),
    (MusicNote, "../../../../../resources/icons/sidebar/audio-export.svg"),
    // 钢琴卷帘右键上下文菜单图标
    (ContextMenuCut, "../../../../../resources/icons/context-menu/cut-notes.svg"),
    (ContextMenuCopy, "../../../../../resources/icons/context-menu/copy-notes.svg"),
    (ContextMenuPaste, "../../../../../resources/icons/context-menu/paste-notes.svg"),
    (ContextMenuDelete, "../../../../../resources/icons/context-menu/delete-item.svg"),
    (ContextMenuSelectAll, "../../../../../resources/icons/context-menu/select-all-notes.svg"),
    (ContextMenuColorPalette, "../../../../../resources/icons/context-menu/set-track-color.svg"),
    (ContextMenuChannel, "../../../../../resources/icons/context-menu/set-midi-channel.svg"),
    (ContextMenuPort, "../../../../../resources/icons/context-menu/set-midi-port.svg"),
    (ContextMenuRecoverTrack, "../../../../../resources/icons/context-menu/recover-deleted-track.svg"),
    // 素材库右键菜单图标
    (ContextMenuUploadToCloud, "../../../../../resources/icons/context-menu/upload-to-cloud.svg"),
    (Mixer, "../../../../../resources/icons/sidebar/mixer.svg"),
    (MixerActive, "../../../../../resources/icons/sidebar/mixer-active.svg"),
}

/// 设置 HiDPI 状态（egui 图标缓存会整帧重绘，这里只记录状态）。
pub fn set_hidpi_enabled(enabled: bool) {
    HIDPI_ENABLED.store(enabled, Ordering::Relaxed);
}
