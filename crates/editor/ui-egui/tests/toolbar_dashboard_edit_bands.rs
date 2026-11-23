//! 仪表盘字段编辑：BPM / PPQ 的进入 / 输入 / 失焦提交 / 取消 / 会话聚焦。
//!
//! 与 `toolbar_controls_bands.rs` 共用 `tests/common/toolbar_harness.rs` 的
//! harness 与几何正推（拆文件只为单文件行数纪律）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Event, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::toolbar as toolbar_view;
use lumino_ui_egui::chrome::toolbar_popup::{self, ToolbarAction};
use lumino_ui_egui::chrome::{DashboardBlur, DashboardField, MetricBlur, ToolbarModel};
use lumino_ui_egui::layout;

mod common;

use common::toolbar_harness::{
    BlurHarness, click, group_x, harness, harness_blur, lower_row_y, mid_y, model,
    playback_centers, upper_row_y,
};

/// 上下两行各自进入自己的编辑态（不再互相误触发）。
#[test]
fn dashboard_rows_enter_their_own_edit() {
    let x = group_x()[4] + layout::DASHBOARD_CELL_W / 2.0;
    for (y, expected) in [
        (upper_row_y(), ToolbarAction::BpmEditToggled),
        (lower_row_y(), ToolbarAction::PpqEditToggled),
    ] {
        let (mut h, actions) = harness(model());
        click(&mut h, Pos2::new(x, y));
        assert_eq!(
            actions.borrow().last(),
            Some(&expected),
            "点击 y={y} 应进入 {expected:?}，实际 {:?}",
            actions.borrow()
        );
    }
}

/// BPM 编辑态输入 → `BpmEditChanged`；PPQ 仍走 `PpqEditChanged`。
#[test]
fn dashboard_edits_emit_field_specific_changes() {
    let cases = [
        (
            ToolbarModel {
                bpm_editing: true,
                ..model()
            },
            false,
        ),
        (
            ToolbarModel {
                ppq_editing: true,
                ..model()
            },
            true,
        ),
    ];
    for (current, is_ppq) in cases {
        let (mut h, actions) = harness(current);
        *h.input_mut() = RawInput {
            events: vec![Event::Text("4".to_owned())],
            ..Default::default()
        };
        h.run();
        let changed = actions.borrow().iter().any(|action| {
            if is_ppq {
                matches!(action, ToolbarAction::PpqEditChanged(_))
            } else {
                matches!(action, ToolbarAction::BpmEditChanged(_))
            }
        });
        assert!(
            changed,
            "编辑{}输入应回传对应 Changed 动作，实际 {:?}",
            if is_ppq { " PPQ" } else { " BPM" },
            actions.borrow()
        );
    }
}

/// BUG 回归：编辑任一字段时点击空白处必须提交（输入框失焦 → `MetricBlur::Commit`）。
#[test]
fn clicking_blank_commits_pending_dashboard_edit() {
    for (field, current) in [
        (
            DashboardField::Bpm,
            ToolbarModel {
                bpm_editing: true,
                ..model()
            },
        ),
        (
            DashboardField::Ppq,
            ToolbarModel {
                ppq_editing: true,
                ..model()
            },
        ),
    ] {
        let mut h = harness_blur(current);
        // 工具栏分组之间的空隙：既不在输入框内，也不压任何按钮。
        click(&mut h.harness, Pos2::new(group_x()[4] - 8.0, mid_y()));
        assert_eq!(
            h.blurs.borrow().as_slice(),
            [DashboardBlur {
                field,
                outcome: MetricBlur::Commit
            }]
            .as_slice(),
            "{field:?} 失焦应提交编辑"
        );
    }
}

/// 失焦提交与同帧按钮点击必须同时送达。
///
/// 失焦信号走独立通道（`DashboardBlur`），不会被单槽工具栏动作覆盖——否则点播放
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
        [DashboardBlur {
            field: DashboardField::Ppq,
            outcome: MetricBlur::Commit
        }]
        .as_slice(),
        "点播放应同时提交 PPQ 编辑"
    );
    assert!(
        matches!(h.actions.borrow().last(), Some(ToolbarAction::Play)),
        "播放动作不得被失焦提交吞掉，实际 {:?}",
        h.actions.borrow()
    );
}

/// 回车同样经失焦通道提交；Esc 则取消（不写入）。BPM / PPQ 行为一致。
#[test]
fn enter_commits_and_escape_cancels_dashboard_edit() {
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

    for field in [DashboardField::Bpm, DashboardField::Ppq] {
        for (key, outcome) in [
            (egui::Key::Enter, MetricBlur::Commit),
            (egui::Key::Escape, MetricBlur::Cancel),
        ] {
            let mut current = model();
            match field {
                DashboardField::Bpm => current.bpm_editing = true,
                DashboardField::Ppq => current.ppq_editing = true,
            }
            let mut h = harness_blur(current);
            press(&mut h, key);
            assert_eq!(
                h.blurs.borrow().as_slice(),
                [DashboardBlur { field, outcome }].as_slice(),
                "{field:?} 的 {key:?} 应产生 {outcome:?}"
            );
        }
    }
}

/// 回归：第二次进入编辑仍要重新抢焦点（BPM / PPQ 各自的会话标记）。
///
/// 修复前「已抢过焦点」的临时标记从不清理，第二个编辑会话的输入框拿不到
/// 键盘——打字无反应，失焦提交也无从触发。
#[test]
fn dashboard_edit_refocuses_on_each_session() {
    for field in [DashboardField::Bpm, DashboardField::Ppq] {
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
                let mut current = model();
                match field {
                    DashboardField::Bpm => current.bpm_editing = editing_w.get(),
                    DashboardField::Ppq => current.ppq_editing = editing_w.get(),
                }
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

        let has_changed = |actions: &RefCell<Vec<ToolbarAction>>| {
            actions.borrow().iter().any(|action| match field {
                DashboardField::Bpm => matches!(action, ToolbarAction::BpmEditChanged(_)),
                DashboardField::Ppq => matches!(action, ToolbarAction::PpqEditChanged(_)),
            })
        };

        type_char(&mut h, "4");
        assert!(
            has_changed(&actions),
            "{field:?} 第一个编辑会话应能输入，实际 {:?}",
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
            has_changed(&actions),
            "{field:?} 第二个编辑会话必须重新聚焦，实际 {:?}",
            actions.borrow()
        );
    }
}
