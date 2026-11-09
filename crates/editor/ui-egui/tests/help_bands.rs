//! Help 窗口的无头交互测试：搜索 / 话题选择 / 全部重现 / 关闭。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, HelpModel, HelpSectionModel, HelpTopicBodyModel,
    HelpTopicRowModel,
};
use lumino_ui_egui::help::HelpAction;
use lumino_ui_egui::icons::IconCache;

fn model() -> HelpModel {
    HelpModel {
        titlebar: false,
        title: "帮助".to_owned(),
        search_hint: "搜索帮助...".to_owned(),
        query: String::new(),
        sections: vec![
            HelpSectionModel {
                title: "基础".to_owned(),
                topics: vec![
                    HelpTopicRowModel {
                        index: 0,
                        label: "框选与多选".to_owned(),
                        active: true,
                    },
                    HelpTopicRowModel {
                        index: 1,
                        label: "缩放与滚动".to_owned(),
                        active: false,
                    },
                ],
            },
            HelpSectionModel {
                title: "编辑".to_owned(),
                topics: vec![HelpTopicRowModel {
                    index: 2,
                    label: "新建音符".to_owned(),
                    active: false,
                }],
            },
        ],
        empty: "没有匹配的话题".to_owned(),
        topic: Some(HelpTopicBodyModel {
            title: "框选与多选".to_owned(),
            tip: "鼠标（框选）".to_owned(),
            paragraphs: vec![
                "按住鼠标左键拖动即可框选。".to_owned(),
                "Shift 追加选择，Alt 从选区中移除。".to_owned(),
            ],
        }),
        reset: "全部重现".to_owned(),
        close: "关闭".to_owned(),
    }
}

fn make_harness(model: HelpModel) -> (Harness<'static>, Rc<RefCell<Vec<HelpAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 620.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::Help(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::help::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn labels_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "帮助",
        "基础",
        "编辑",
        "缩放与滚动",
        "新建音符",
        "按住鼠标左键拖动即可框选。",
        "Shift 追加选择，Alt 从选区中移除。",
        "全部重现",
        "关闭",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    // 当前话题标题同时出现在列表行与正文标题里（两个节点）。
    assert!(
        harness.query_all_by_label("框选与多选").count() >= 2,
        "当前话题应同时出现在列表与正文"
    );
}

#[test]
fn topic_click_dispatches_index() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("新建音符").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HelpAction::TopicSelected(2)));
}

#[test]
fn search_input_dispatches_query() {
    let (mut harness, actions) = make_harness(model());
    let input = harness
        .get_by(|node| node.role() == egui::Role::TextInput && node.value().as_deref() == Some(""));
    input.focus();
    input.type_text("brush");
    harness.run();
    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(
            |action| matches!(action, HelpAction::QueryChanged(text) if text.contains("brush"))
        ),
        "搜索框应回传 QueryChanged，实际 {dispatched:?}"
    );
}

#[test]
fn buttons_dispatch_reset_and_close() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("全部重现").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HelpAction::ResetTips));

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HelpAction::Close));
}

/// 无匹配时列表与正文都显示空文案。
#[test]
fn empty_state_is_visible() {
    let mut model = model();
    model.sections.clear();
    model.topic = None;
    let (harness, _) = make_harness(model);
    // 列表与正文各一个。
    assert_eq!(harness.query_all_by_label("没有匹配的话题").count(), 2);
}
