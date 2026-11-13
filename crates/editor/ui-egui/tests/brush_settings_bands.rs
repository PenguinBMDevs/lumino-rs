//! 画刷设置对话框的无头测试：逐层下拉 / 增删层 / 保存取消 / 空态。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use lumino_ui_egui::brush_settings::BrushSettingsAction;
use lumino_ui_egui::chrome::{
    self, BrushLevelModel, BrushSettingsModel, BrushTrackChoice, ChromeModel, DialogModel,
};
use lumino_ui_egui::icons::IconCache;

fn level(index: usize, assigned: Option<usize>, can_remove: bool) -> BrushLevelModel {
    BrushLevelModel {
        index,
        label: format!("第 {} 层", index + 1),
        tracks: vec![
            BrushTrackChoice {
                id: None,
                label: "默认（自动分配）".to_owned(),
                selected: assigned.is_none(),
            },
            BrushTrackChoice {
                id: Some(0),
                label: "钢琴".to_owned(),
                selected: assigned == Some(0),
            },
            BrushTrackChoice {
                id: Some(1),
                label: "弦乐".to_owned(),
                selected: assigned == Some(1),
            },
        ],
        can_remove,
    }
}

fn model(levels: Vec<BrushLevelModel>) -> BrushSettingsModel {
    BrushSettingsModel {
        titlebar: false,
        title: "画刷绘制行为设置".to_owned(),
        levels,
        has_tracks: true,
        empty: "无可用音轨，请先在工程中创建普通音轨。".to_owned(),
        save: "保存".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: BrushSettingsModel,
) -> (Harness<'static>, Rc<RefCell<Vec<BrushSettingsAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 420.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::BrushSettings(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::brush_settings::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn levels_and_buttons_are_visible() {
    let (harness, _) = make_harness(model(vec![level(0, None, false), level(1, Some(1), false)]));
    for label in ["画刷绘制行为设置", "第 1 层", "第 2 层", "保存", "取消"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn save_and_cancel_dispatch() {
    let (mut harness, actions) = make_harness(model(vec![level(0, None, false)]));
    harness.get_by_label("保存").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&BrushSettingsAction::Save));

    let (mut harness, actions) = make_harness(model(vec![level(0, None, false)]));
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&BrushSettingsAction::Cancel));
}

/// 下拉改绑音轨：回传层下标与音轨 id。
#[test]
fn track_combo_dispatches_level_and_id() {
    let (mut harness, actions) = make_harness(model(vec![level(0, None, false)]));
    harness
        .get_by(|node| node.role() == egui::Role::ComboBox)
        .click();
    harness.run();
    harness.run();
    harness
        .get_all_by_label("弦乐")
        .last()
        .expect("下拉里应有「弦乐」")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&BrushSettingsAction::LevelTrackChanged(0, Some(1)))
    );
}

/// 只有可删除的层会回传 RemoveLevel；单层时「删除层」不可交互。
#[test]
fn remove_respects_minimum_thickness() {
    // 单层：删除按钮存在但标记为禁用。
    let (harness, _) = make_harness(model(vec![level(0, None, false)]));
    let remove = harness.get_by_label("删除层");
    assert!(
        remove.accesskit_node().is_disabled(),
        "单层时删除按钮应为禁用"
    );

    // 两层：第 1 行的删除按钮可点，回传 RemoveLevel(0)。
    let (mut harness, actions) =
        make_harness(model(vec![level(0, None, true), level(1, None, true)]));
    harness
        .get_all_by_label("删除层")
        .next()
        .expect("两层时应有删除按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&BrushSettingsAction::RemoveLevel(0))
    );
}

/// 「添加层」回传当前层下标。
#[test]
fn add_level_dispatches_index() {
    let (mut harness, actions) = make_harness(model(vec![level(0, None, false)]));
    harness
        .get_all_by_label("添加层")
        .next()
        .expect("应有添加按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&BrushSettingsAction::AddLevel(0))
    );
}

/// 无可用音轨时显示禁用提示，列表不渲染。
#[test]
fn empty_state_shows_hint() {
    let mut model = model(Vec::new());
    model.has_tracks = false;
    let (harness, _) = make_harness(model);
    assert!(
        harness
            .query_by_label("无可用音轨，请先在工程中创建普通音轨。")
            .is_some()
    );
}
