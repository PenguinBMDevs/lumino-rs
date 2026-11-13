//! 工程设置对话框的无头交互测试：按钮动作 + 文本输入回传。
//!
//! 窗口按真实栅格（450 宽）渲染，但测试高度放大到 1000，保证整张表单
//! （生产里需要滚动）全部实例化，便于按可访问性节点定位。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, ProjectNoteCountModel, ProjectSettingsModel,
};
use lumino_ui_egui::project_settings::ProjectSettingsAction;

fn model() -> ProjectSettingsModel {
    ProjectSettingsModel {
        titlebar: false,
        title: "工程信息设置".to_owned(),
        name_label: "项目名称".to_owned(),
        name: "我的工程".to_owned(),
        name_hint: "无标题".to_owned(),
        bpm_label: "BPM 速度".to_owned(),
        bpm: "120".to_owned(),
        copyright_label: "版权信息".to_owned(),
        copyright: "© 2026".to_owned(),
        copyright_hint: "输入版权信息（可选）".to_owned(),
        author_label: "作者".to_owned(),
        author: "张三".to_owned(),
        author_hint: "输入作者（可选）".to_owned(),
        time_signature_label: "拍号".to_owned(),
        numerator: "4".to_owned(),
        denominator: "4".to_owned(),
        use10: false,
        use10_label: "启用通道 10".to_owned(),
        use10_hint: "区域色号上限 16".to_owned(),
        created_label: "创建日期".to_owned(),
        created: "2026-07-01".to_owned(),
        editing_time_label: "累计创作时间".to_owned(),
        editing_time: "1 分钟 6 秒".to_owned(),
        note_count: Some(ProjectNoteCountModel {
            label: "音符总量".to_owned(),
            value: "1,234".to_owned(),
        }),
        ok: "确定".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: ProjectSettingsModel,
) -> (Harness<'static>, Rc<RefCell<Vec<ProjectSettingsAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(450.0, 1000.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::ProjectSettings(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::project_settings::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 确定 / 取消按钮各自回传对应动作。
#[test]
fn buttons_dispatch_confirm_and_cancel() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("确定").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ProjectSettingsAction::Confirm)
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ProjectSettingsAction::Cancel)
    );
}

/// 名称输入框（按当前值定位）接受输入并回传 `NameChanged`（原样带回新文本）。
#[test]
fn name_input_dispatches_new_text() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("我的工程")
    });
    input.focus();
    input.type_text("X");
    harness.run();

    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(|action| matches!(
            action,
            ProjectSettingsAction::NameChanged(text) if text.contains('X')
        )),
        "名称输入应回传 NameChanged，实际 {dispatched:?}"
    );
}

/// 只读行与按钮文案都进入可访问树（不依赖像素）。
#[test]
fn readonly_rows_and_labels_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "工程信息设置",
        "项目名称",
        "BPM 速度",
        "版权信息",
        "作者",
        "拍号",
        "创建日期",
        "累计创作时间",
        "音符总量",
        "1,234",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}
