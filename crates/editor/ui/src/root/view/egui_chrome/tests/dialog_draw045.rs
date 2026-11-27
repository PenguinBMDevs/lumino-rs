//! DRAW-045：素材删除确认 / 自定义 BPM 上限的整窗模型与「主窗口无覆盖层」。
//!
//! 覆盖三件事：
//! 1. 两个对话框类型必须产出各自的整窗 `DialogModel`（而不是落到空容器）；
//! 2. 主窗口即使还挂着 `pending_delete`，也不得再生成模态覆盖层；
//! 3. 两个类型在 `dialog_size_hint` 里都有钉死尺寸（可被 `ALL_DIALOGS` 检查复用）。

use super::*;
use crate::state::root_state::DialogType;
use crate::view::dialog_preferred_size::{dialog_size_hint, static_window_size};
use lumino_ui_core::geom::Size;

fn dialog_root(dialog: DialogType) -> Root {
    Root::new_dialog(&lumino_ui_core::Theme::TokyoNightStorm.to_string(), dialog)
}

#[test]
fn material_delete_renders_dialog_model_with_index_and_name() {
    let mut root = dialog_root(DialogType::MaterialDelete);
    root.set_material_delete_dialog(3, "钢琴音色.json".to_string());

    let model = root.egui_chrome_model();
    let Some(DialogModel::MaterialDelete(delete)) = model.dialog else {
        panic!("素材删除确认必须以整窗对话框模型渲染");
    };
    assert_eq!(delete.index, 3);
    assert_eq!(delete.name, "钢琴音色.json");
    assert!(model.titlebar.is_none(), "对话框窗口不得绘制主窗口外壳");
}

#[test]
fn tempo_max_bpm_renders_dialog_model_with_prefilled_input() {
    let mut root = dialog_root(DialogType::TempoMaxBpm);
    root.set_tempo_max_bpm_dialog(700.0);

    let model = root.egui_chrome_model();
    let Some(DialogModel::TempoMaxBpm(dialog)) = model.dialog else {
        panic!("自定义 BPM 上限必须以整窗对话框模型渲染");
    };
    assert_eq!(dialog.input, "700", "打开时用当前上限预填");
    assert!(dialog.error.is_none());
}

#[test]
fn main_window_no_longer_renders_material_delete_overlay() {
    // 旧实现的模态层由 `right_sidebar.materials.pending_delete` 驱动；
    // 迁移后该字段只作为回执配对依据，主窗口不得再产出任何对话框模型。
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.right_sidebar.materials.pending_delete = Some(0);
    root.right_sidebar.materials.pending_delete_name = Some("钢琴音色.json".to_string());

    let model = root.egui_chrome_model();
    assert!(
        model.dialog.is_none(),
        "主窗口不得再有全屏遮罩 + 居中卡片（DRAW-045）"
    );
}

#[test]
fn new_dialog_types_declare_pinned_window_sizes() {
    let work_area = Size::new(1920.0, 1040.0);
    for dialog in [DialogType::MaterialDelete, DialogType::TempoMaxBpm] {
        let size = static_window_size(dialog_size_hint(dialog), work_area);
        assert!(
            size.width > 0.0 && size.height > 0.0,
            "{dialog:?} 必须声明可用的窗口尺寸"
        );
    }
}
