//! 仪表盘 PPQ 编辑：进入 / 输入 / 失焦提交 / 取消 / 会话聚焦。
//!
//! 与 `toolbar_controls_bands.rs` 共用 `tests/common/toolbar_harness.rs` 的
//! harness 与几何正推（拆文件只为单文件行数纪律）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Event, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::PpqBlur;
use lumino_ui_egui::chrome::ToolbarModel;
use lumino_ui_egui::chrome::toolbar as toolbar_view;
use lumino_ui_egui::chrome::toolbar_popup::{self, ToolbarAction};
use lumino_ui_egui::layout;

mod common;

use common::toolbar_harness::{
    BlurHarness, click, group_x, harness, harness_blur, lower_row_y, mid_y, model,
    playback_centers, upper_row_y,
};

#[test]
fn ppq_cell_enters_edit_and_edit_emits_changes() {
    let cell = Pos2::new(group_x()[4] + layout::DASHBOARD_CELL_W / 2.0, lower_row_y());

    let (mut h, actions) = harness(model());
    click(&mut h, cell);
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::PpqEditToggled)
    ));

    // 编辑态：输入一个字符 → PpqEditChanged。
    let (mut h, actions) = harness(ToolbarModel {
        ppq_editing: true,
        ..model()
    });
    *h.input_mut() = RawInput {
        events: vec![Event::Text("4".to_owned())],
        ..Default::default()
    };
    h.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::PpqEditChanged(_))),
        "输入应回传 PpqEditChanged，实际 {:?}",
        actions.borrow()
    );
}

/// BUG 回归：BPM 是只读显示，点击它不得进入 PPQ 编辑。
///
/// 修复前第一列上下两行（BPM + PPQ）共用一个 `Sense::click()` 命中区，
/// 点 BPM 会误触发 `PpqEditToggled`。
#[test]
fn bpm_row_click_does_not_enter_ppq_edit() {
    let x = group_x()[4] + layout::DASHBOARD_CELL_W / 2.0;
    let (mut h, actions) = harness(model());
    click(&mut h, Pos2::new(x, upper_row_y()));
    assert!(
        actions.borrow().is_empty(),
        "点击 BPM 不得产生工具栏动作，实际 {:?}",
        actions.borrow()
    );
}

/// BUG 回归：编辑 PPQ 时点击空白处必须提交（输入框失焦 → `PpqBlur::Commit`）。
#[test]
fn clicking_blank_commits_pending_ppq_edit() {
    let mut h = harness_blur(ToolbarModel {
        ppq_editing: true,
        ..model()
    });
    // 工具栏分组之间的空隙：既不在 PPQ 输入框内，也不压任何按钮。
    let blank = Pos2::new(group_x()[4] - 8.0, mid_y());
    click(&mut h.harness, blank);
    assert_eq!(
        h.blurs.borrow().as_slice(),
        [PpqBlur::Commit].as_slice(),
        "点击空白处应提交 PPQ 编辑"
    );
}

/// 失焦提交与同帧按钮点击必须同时送达。
///
/// 失焦信号走独立通道（`PpqBlur`），不会被单槽工具栏动作覆盖——否则点播放
/// 只会提交编辑、播放动作被吞掉（或反之，编辑被静默丢弃）。
#[test]
fn blur_commit_keeps_same_frame_button_click() {
    let play = Pos2::new(playback_centers()[1], mid_y());
    let mut h = harness_blur(ToolbarModel {
        ppq_editing: true,
        ..model()
    });
    click(&mut h.harness, play);
    assert_eq!(
        h.blurs.borrow().as_slice(),
        [PpqBlur::Commit].as_slice(),
        "点播放应同时提交 PPQ 编辑"
    );
    assert!(
        matches!(h.actions.borrow().last(), Some(ToolbarAction::Play)),
        "播放动作不得被失焦提交吞掉，实际 {:?}",
        h.actions.borrow()
    );
}

/// 回车同样经失焦通道提交；Esc 则取消（不写入）。
#[test]
fn enter_commits_and_escape_cancels_ppq_edit() {
    fn press(h: &mut BlurHarness, key: egui::Key) {
        *h.harness.input_mut() = RawInput {
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        h.harness.run();
    }

    for (key, expected) in [
        (egui::Key::Enter, PpqBlur::Commit),
        (egui::Key::Escape, PpqBlur::Cancel),
    ] {
        let mut h = harness_blur(ToolbarModel {
            ppq_editing: true,
            ..model()
        });
        press(&mut h, key);
        assert_eq!(
            h.blurs.borrow().as_slice(),
            [expected].as_slice(),
            "{key:?} 应产生 {expected:?}"
        );
    }
}

/// 回归：第二次进入 PPQ 编辑仍要重新抢焦点。
///
/// 修复前「已抢过焦点」的临时标记从不清理，第二个编辑会话的输入框拿不到
/// 键盘——打字无反应，失焦提交也无从触发。
#[test]
fn ppq_edit_refocuses_on_each_session() {
    let editing = Rc::new(Cell::new(true));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let editing_w = editing.clone();
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, layout::TOOLBAR_H))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            let current = ToolbarModel {
                ppq_editing: editing_w.get(),
                ..model()
            };
            toolbar_view::show_model(ui, &current, &mut icons);
            if let Some(action) = toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    h.run();

    fn type_char(h: &mut Harness<'static>, text: &str) {
        *h.input_mut() = RawInput {
            events: vec![Event::Text(text.to_owned())],
            ..Default::default()
        };
        h.run();
    }

    type_char(&mut h, "4");
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::PpqEditChanged(_))),
        "第一个编辑会话应能输入，实际 {:?}",
        actions.borrow()
    );

    // 退出编辑（生产侧确认后模型翻转为 false）→ 再进入。
    actions.borrow_mut().clear();
    editing.set(false);
    h.run();
    editing.set(true);
    h.run();

    type_char(&mut h, "5");
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::PpqEditChanged(_))),
        "第二个编辑会话必须重新聚焦，实际 {:?}",
        actions.borrow()
    );
}
