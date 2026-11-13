//! 找回删除音轨对话框的无头测试：列表选择 / 恢复 / 永久删除 / 关闭 / 空态。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, RecoverTrackModel, RecoverTrackRowModel, RecoverTrackSelection,
};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::recover_track::RecoverTrackAction;

fn entry(filename: &str, deleted: &str, notes: u32, selected: bool) -> RecoverTrackRowModel {
    RecoverTrackRowModel {
        filename: filename.to_owned(),
        deleted_at: deleted.to_owned(),
        note_count: notes.to_string(),
        selected,
    }
}

fn model(selected: bool) -> RecoverTrackModel {
    RecoverTrackModel {
        titlebar: false,
        title: "找回删除音轨".to_owned(),
        hint: "选中条目后点击「恢复」可还原至原位置；「永久删除」将销毁缓存文件并释放轨道编号。"
            .to_owned(),
        filename_header: "文件名".to_owned(),
        deleted_header: "删除时间".to_owned(),
        notes_header: "音符总数".to_owned(),
        entries: vec![
            entry("track3.lmdeltrack", "2026-11-01 12:00", 1200, selected),
            entry("track7.lmdeltrack", "2026-11-02 09:30", 42, false),
        ],
        empty: "暂无已删除的音轨缓存".to_owned(),
        selected: selected.then(|| RecoverTrackSelection {
            path: "C:/cache/track3.lmdeltrack".to_owned(),
            original_index: 3,
            track_id: 3,
        }),
        can_act: selected,
        restore: "恢复".to_owned(),
        delete: "永久删除".to_owned(),
        close: "关闭".to_owned(),
    }
}

fn make_harness(
    model: RecoverTrackModel,
) -> (Harness<'static>, Rc<RefCell<Vec<RecoverTrackAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(820.0, 640.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::RecoverTrack(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::recover_track::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn header_rows_and_buttons_are_visible() {
    let (harness, _) = make_harness(model(true));
    for label in [
        "找回删除音轨",
        "文件名",
        "删除时间",
        "音符总数",
        "恢复",
        "永久删除",
        "关闭",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    // 两行文件名：行按钮与单元格各一个节点（同文本）。
    for name in ["track3.lmdeltrack", "track7.lmdeltrack"] {
        assert!(
            harness.query_all_by_label(name).count() >= 1,
            "缺少行文本：{name}"
        );
    }
}

#[test]
fn row_click_dispatches_selection() {
    let (mut harness, actions) = make_harness(model(false));
    harness
        .get_all_by_label("track7.lmdeltrack")
        .next()
        .expect("行应可定位")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RecoverTrackAction::SelectionChanged(1))
    );
}

#[test]
fn restore_and_delete_carry_selection_payload() {
    let (mut harness, actions) = make_harness(model(true));
    harness.get_by_label("恢复").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RecoverTrackAction::Restore {
            path: "C:/cache/track3.lmdeltrack".to_owned(),
            original_index: 3,
        })
    );

    let (mut harness, actions) = make_harness(model(true));
    harness.get_by_label("永久删除").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RecoverTrackAction::PermanentlyDelete {
            path: "C:/cache/track3.lmdeltrack".to_owned(),
            track_id: 3,
        })
    );
}

#[test]
fn close_dispatches() {
    let (mut harness, actions) = make_harness(model(false));
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RecoverTrackAction::CloseDialog)
    );
}

#[test]
fn empty_state_is_visible() {
    let mut model = model(false);
    model.entries.clear();
    let (harness, _) = make_harness(model);
    assert!(harness.query_by_label("暂无已删除的音轨缓存").is_some());
}
