//! 右键菜单条目 → 图标映射。
//!
//! 文字已经承担区分职责，这里的图标只做**语义锚点**：能用现有字形表达就用，
//! 只有真正的设置类动作才用齿轮。图标库（`resources/icons/context-menu`）只有
//! 10 个字形，硬凑会出现 8 个动作共用齿轮的情况——那是 DRAW-046 改用行式菜单的
//! 直接原因，不要为了「每个条目一个图标」再把齿轮当兜底。

use lumino_message::PianoRollContextMenuItem as Item;

use crate::icons::Icon;

/// 菜单项 → egui 图标。
pub(super) fn item_icon(item: Item) -> Icon {
    match item {
        Item::Cut => Icon::ContextMenuCut,
        Item::Copy => Icon::ContextMenuCopy,
        Item::Paste => Icon::ContextMenuPaste,
        Item::Delete | Item::DeleteDrawnShape => Icon::ContextMenuDelete,
        Item::SelectAll => Icon::ContextMenuSelectAll,
        Item::BatchEdit => Icon::PencilOutline,
        Item::ShapeList => Icon::FolderTree,
        Item::PasteShapeText => Icon::TextInput,
        // 形状组合
        Item::LiveShape => Icon::Curve,
        Item::JoinShapes => Icon::Glue,
        Item::MergeShapes => Icon::Tie,
        Item::EditShape => Icon::Pencil,
        Item::PaintAreas => Icon::PaintBucket,
        Item::MaskToggle | Item::MaskApply => Icon::Eye,
        Item::MaskSettings => Icon::Gear,
        // 对称 / 切割
        Item::SymmetryVertical => Icon::FlipVertical,
        Item::SymmetryHorizontal => Icon::FlipHorizontal,
        Item::SliceCut => Icon::Split,
        // 刀 / 饼干模
        Item::MouldSetKnife => Icon::ContextMenuCut,
        Item::MouldClearKnife => Icon::CancelCross,
        Item::MouldShapes => Icon::ShapeCircle,
        // 派生
        Item::RoundCorners => Icon::ArcTool,
        Item::BetweenCreate => Icon::ArrowsLeftRight,
        Item::CopyAlong => Icon::WaveForm,
        Item::VelocityUp => Icon::TransposeUp,
        Item::CopyShapeText => Icon::ContextMenuCopy,
        Item::ReturnToDrawer => Icon::Undo,
        // 上一代遗留变体：这些条目不再进入 `menu_columns`，保留兜底映射只为穷尽 match。
        Item::SplitLiveShape | Item::SplitJoinedShape => Icon::Undo,
        Item::Straighten => Icon::Quantize,
        Item::StraightenOff => Icon::Ban,
    }
}
