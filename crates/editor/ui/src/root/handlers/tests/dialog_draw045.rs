//! DRAW-045：素材删除确认 / 自定义 BPM 上限对话框的结果分派。
//!
//! 对话框 Root 只产出 `DialogResult`；文件删除与设置写回由 Runner 分派，
//! 这里锁定「确认 / 取消 / 非法输入」三种语义。

use super::*;
use crate::host::DialogResult;
use crate::message::{MaterialDeleteDialogAction, TempoMaxBpmDialogAction};
use crate::root::Root;
use crate::state::root_state::DialogType;

fn dialog_root(dialog: DialogType) -> Root {
    Root::new_dialog(&lumino_ui_core::Theme::TokyoNightStorm.to_string(), dialog)
}

#[test]
fn material_delete_confirm_yields_index_result_and_closes() {
    let mut root = dialog_root(DialogType::MaterialDelete);
    root.set_material_delete_dialog(4, "鼓组".to_string());

    root.update(Message::MaterialDelete(
        MaterialDeleteDialogAction::Confirm(4),
    ));

    assert!(
        matches!(
            root.take_dialog_result(),
            Some(DialogResult::MaterialDeleteConfirm { index: 4 })
        ),
        "确认必须回传待删除素材下标"
    );
    assert!(
        !root.state.material_delete_dialog.is_open,
        "确认后对话框状态应关闭"
    );
}

#[test]
fn material_delete_cancel_yields_cancel_result() {
    let mut root = dialog_root(DialogType::MaterialDelete);
    root.set_material_delete_dialog(1, "弦乐".to_string());

    root.update(Message::MaterialDelete(MaterialDeleteDialogAction::Cancel));

    assert!(matches!(
        root.take_dialog_result(),
        Some(DialogResult::Cancel)
    ));
    assert!(!root.state.material_delete_dialog.is_open);
}

#[test]
fn tempo_confirm_applies_parsed_value_and_closes() {
    let mut root = dialog_root(DialogType::TempoMaxBpm);
    root.set_tempo_max_bpm_dialog(512.0);
    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Input(
        "1234".to_string(),
    )));

    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Confirm));

    match root.take_dialog_result() {
        Some(DialogResult::TempoMaxBpm { bpm }) => {
            assert!((bpm - 1234.0).abs() < f64::EPSILON, "确认值应原样回传");
        }
        other => panic!("确认应产出 TempoMaxBpm 结果，实际 {other:?}"),
    }
    assert!(!root.state.tempo_max_bpm_dialog.is_open);
}

#[test]
fn tempo_confirm_invalid_keeps_dialog_open_with_error() {
    let mut root = dialog_root(DialogType::TempoMaxBpm);
    root.set_tempo_max_bpm_dialog(512.0);
    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Input(
        "abc".to_string(),
    )));

    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Confirm));

    assert!(root.take_dialog_result().is_none(), "非法输入不得产出结果");
    assert!(
        root.state.tempo_max_bpm_dialog.is_open,
        "非法输入必须保留窗口以便修正"
    );
    assert!(
        root.state.tempo_max_bpm_dialog.error.is_some(),
        "必须给出可读错误提示"
    );
}

#[test]
fn tempo_input_clears_previous_error() {
    let mut root = dialog_root(DialogType::TempoMaxBpm);
    root.set_tempo_max_bpm_dialog(512.0);
    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Input(
        "abc".to_string(),
    )));
    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Confirm));
    assert!(root.state.tempo_max_bpm_dialog.error.is_some());

    root.update(Message::TempoMaxBpm(TempoMaxBpmDialogAction::Input(
        "2048".to_string(),
    )));
    assert!(
        root.state.tempo_max_bpm_dialog.error.is_none(),
        "继续输入时应清除上一次的错误提示"
    );
}
