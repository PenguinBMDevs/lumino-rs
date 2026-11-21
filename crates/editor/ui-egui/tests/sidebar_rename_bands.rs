//! 左侧栏行内重命名：点击别处提交 / Esc 取消 / 回车只提交一次 / 换轨先保存。
//!
//! 真机回归点：重命名输入新名字后**点击空白处不保存**，输入框与草稿停在原地；
//! 只有回车生效，Esc 也没有取消路径。根因是行内只在 `lost_focus() && Enter` 时
//! 发动作，而 egui 把「点击别处」「Esc」都表达成失焦。
//!
//! 另外钉死「失焦与同帧控件点击互不吞并」：确认走独立通道（`take_rename_blur`），
//! 点另一条音轨时 `SelectTrack` 仍然送达。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, Key, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::sidebar::{self, RenameBlur, SidebarPanelAction, SidebarRenameBlur};
use lumino_ui_egui::chrome::{SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack};
use lumino_ui_egui::layout;

/// 正在重命名的音轨 ID（故意不是 0..n，验证回传的是音轨 ID 而不是下标）。
const RENAMING: usize = 3;
const RENAMING_ROW: usize = 1;

fn track(id: usize, label: &str, name: &str, selected: bool) -> SidebarTrack {
    SidebarTrack {
        id,
        label: label.to_owned(),
        name: name.to_owned(),
        color: None,
        selected,
        conductor: false,
        soloed: false,
        renaming: false,
        rename_buffer: String::new(),
    }
}

fn model() -> SidebarModel {
    SidebarModel {
        panel_visible: true,
        panel_width: 200.0,
        active_group: Some(SidebarGroup::PianoRoll),
        file_active: true,
        panel: SidebarPanel::Tracks,
        tracks: vec![
            track(7, "A01", "Setup", false),
            track(RENAMING, "A02", "Piano", true),
            track(11, "A03", "Strings", false),
        ],
        track_list_title: "音轨列表".to_owned(),
        add_track_label: "添加音轨".to_owned(),
        ..SidebarModel::default()
    }
}

/// 第 `RENAMING_ROW` 行进入重命名态，草稿 = 当前名字。
fn renaming_model() -> SidebarModel {
    let mut model = model();
    let track = &mut model.tracks[RENAMING_ROW];
    track.renaming = true;
    track.rename_buffer = track.name.clone();
    model
}

struct RenameHarness {
    harness: Harness<'static>,
    actions: Rc<RefCell<Vec<SidebarPanelAction>>>,
    blurs: Rc<RefCell<Vec<SidebarRenameBlur>>>,
}

fn harness(initial: SidebarModel) -> RenameHarness {
    // 模型放 `Rc<RefCell>`：模拟生产把 `RenameChanged` 写回草稿，
    // 下一帧输入框读到的是最新文本（否则失焦时读到的永远是初始草稿）。
    let model = Rc::new(RefCell::new(initial));
    let model_ui = model.clone();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_ui = actions.clone();
    let blurs = Rc::new(RefCell::new(Vec::new()));
    let blurs_ui = blurs.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(layout::SIDEBAR_W, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            let current = model_ui.borrow().clone();
            sidebar::show_model(ui, &current, &mut icons);
            if let Some(action) = sidebar::take_panel_action(ui.ctx()) {
                if let SidebarPanelAction::RenameChanged { track_id, value } = &action {
                    let mut model = model_ui.borrow_mut();
                    if let Some(track) = model.tracks.iter_mut().find(|track| track.id == *track_id)
                    {
                        track.rename_buffer = value.clone();
                    }
                }
                actions_ui.borrow_mut().push(action);
            }
            if let Some(blur) = sidebar::take_rename_blur(ui.ctx()) {
                blurs_ui.borrow_mut().push(blur);
            }
        });
    harness.run();
    RenameHarness {
        harness,
        actions,
        blurs,
    }
}

fn click(h: &mut RenameHarness, pos: Pos2) {
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *h.harness.input_mut() = input;
    h.harness.run();
}

fn type_text(h: &mut RenameHarness, text: &str) {
    *h.harness.input_mut() = RawInput {
        events: vec![Event::Text(text.to_owned())],
        ..Default::default()
    };
    h.harness.run();
}

fn press(h: &mut RenameHarness, key: Key) {
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

/// 第 `index` 行名称区（也是重命名输入框所在列）。
fn row_center(index: usize) -> Pos2 {
    let top = layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H * index as f32;
    Pos2::new(
        layout::ROUTE_BAR_W + 8.0 + 80.0,
        top + layout::TRACK_ROW_H / 2.0,
    )
}

/// 面板内、行列表下方的空白（不属于任何行/输入框）。
fn blank() -> Pos2 {
    Pos2::new(layout::ROUTE_BAR_W + 60.0, 420.0)
}

fn draft(text: &str, outcome: RenameBlur) -> SidebarRenameBlur {
    SidebarRenameBlur {
        track_id: RENAMING,
        value: text.to_owned(),
        outcome,
    }
}

/// 输入新名字后点击面板空白：必须提交（本卡核心回归点）。
#[test]
fn clicking_blank_commits_the_typed_draft() {
    let mut h = harness(renaming_model());
    type_text(&mut h, "X");
    assert_eq!(
        h.actions.borrow().last(),
        Some(&SidebarPanelAction::RenameChanged {
            track_id: RENAMING,
            value: "PianoX".to_owned(),
        }),
        "输入应先落到草稿，实际 {:?}",
        h.actions.borrow()
    );

    h.actions.borrow_mut().clear();
    click(&mut h, blank());

    assert_eq!(
        h.blurs.borrow().as_slice(),
        &[draft("PianoX", RenameBlur::Commit)],
        "点击空白必须提交最新草稿（含刚敲的字）"
    );
    assert!(
        h.actions.borrow().is_empty(),
        "提交走独立失焦通道，不应再占用单槽面板动作：{:?}",
        h.actions.borrow()
    );
}

/// Esc：放弃修改（上游据此恢复原名）。
#[test]
fn escape_cancels_the_draft() {
    let mut h = harness(renaming_model());
    type_text(&mut h, "X");
    press(&mut h, egui::Key::Escape);

    assert_eq!(
        h.blurs.borrow().as_slice(),
        &[draft("PianoX", RenameBlur::Cancel)],
        "Esc 必须上报取消，而不是提交"
    );
}

/// 回车：提交且只提交一次（egui 0.37 的 `lost_focus` 会连续两帧为真）。
#[test]
fn enter_commits_exactly_once() {
    let mut h = harness(renaming_model());
    type_text(&mut h, "X");
    press(&mut h, egui::Key::Enter);
    // 空帧：`lost_focus` 在下一帧仍然为真，去重标记必须挡住第二次上报。
    h.harness.run();

    assert_eq!(
        h.blurs.borrow().as_slice(),
        &[draft("PianoX", RenameBlur::Commit)],
        "回车应恰好提交一次，实际 {:?}",
        h.blurs.borrow()
    );
}

/// 点另一条音轨：先提交当前草稿，同帧的选中动作不得被吞掉。
#[test]
fn clicking_another_track_commits_and_selects() {
    let mut h = harness(renaming_model());
    type_text(&mut h, "X");
    h.actions.borrow_mut().clear();

    click(&mut h, row_center(0));

    assert_eq!(
        h.blurs.borrow().as_slice(),
        &[draft("PianoX", RenameBlur::Commit)],
        "点另一条音轨应先提交当前草稿"
    );
    assert_eq!(
        h.actions.borrow().as_slice(),
        &[SidebarPanelAction::SelectTrack(7)],
        "同一帧的选中动作不得被失焦提交吞掉"
    );
}

/// 点击输入框内部不提交，且编辑会话继续（能继续打字）。
#[test]
fn clicking_inside_the_input_keeps_the_session() {
    let mut h = harness(renaming_model());
    click(&mut h, row_center(RENAMING_ROW));
    assert!(
        h.blurs.borrow().is_empty(),
        "输入框内部点击不得提交：{:?}",
        h.blurs.borrow()
    );

    h.actions.borrow_mut().clear();
    type_text(&mut h, "Y");
    assert!(
        h.actions.borrow().iter().any(|action| matches!(
            action,
            SidebarPanelAction::RenameChanged { track_id, value }
                if *track_id == RENAMING && value.contains('Y')
        )),
        "点回输入框后应继续编辑，实际 {:?}",
        h.actions.borrow()
    );
}
