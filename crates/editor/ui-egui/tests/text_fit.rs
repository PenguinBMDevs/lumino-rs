//! 全局文本适配回归：长文案不得横向越出所在容器。
//!
//! 取 `Context::run` 的绘制输出（`FullOutput::shapes`）逐条检查文本包围盒：
//! - 设置面板：菜单 / 表单 / 说明文字必须收在卡片内；
//! - 保存确认：长提示必须折行，不得压到按钮或越出窗口；
//! - 侧栏素材 / 音轨名称等单行槽位：超出时必须出现省略号（宽度不超过槽位）。
//!
//! 这些断言只看几何，不依赖字体渲染结果，因此没有像素基线那么脆。

use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, DialogPalette, SaveConfirmModel, SettingsLabels,
    SettingsPageModel, SettingsPalette, SettingsSnapshot, UiField, UiPageModel, UiRow,
};
use lumino_ui_egui::icons::IconCache;

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 800.0;
/// 设置卡片右缘：1200 − 外边距 16 − 容器内边距 20。
const CARD_RIGHT: f32 = 1164.0;
/// 设置卡片左缘：16 + 20 + 菜单 200 + 间距 16。
const CARD_LEFT: f32 = 252.0;
/// 设置卡片下缘：800 − 外边距 16 − 确认按钮 34 − 间距 10。
const CARD_BOTTOM: f32 = 740.0;
/// 菜单文字右缘：菜单右缘 236 − 箭头 26 − 间距 8。
const MENU_TEXT_RIGHT: f32 = 202.0;

fn labels() -> SettingsLabels {
    SettingsLabels {
        menu: vec![
            "一个非常非常长的设置菜单名称，用来验证省略号".to_owned(),
            "常规".to_owned(),
            "音频".to_owned(),
            "界面".to_owned(),
            "快捷键".to_owned(),
            "洋葱皮".to_owned(),
            "调色板".to_owned(),
            "编辑".to_owned(),
            "云管理".to_owned(),
            "兼容性".to_owned(),
        ],
        title: "常规".to_owned(),
        eraser_label: "橡皮擦行为:".to_owned(),
        eraser_options: vec!["默认".to_owned(), "直接框选".to_owned()],
        track_add_label: "添加音轨时:".to_owned(),
        track_add_options: vec!["自动跳转".to_owned(), "保持当前".to_owned()],
        log_section: "日志".to_owned(),
        log_label: "日志文件保留份数:".to_owned(),
        log_placeholder: "10".to_owned(),
        confirm: "确认".to_owned(),
    }
}

fn snapshot(page: SettingsPageModel) -> ChromeModel {
    ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels: labels(),
            selected: 0,
            eraser: "默认".to_owned(),
            track_add: "自动跳转".to_owned(),
            log_retention: "10".to_owned(),
            page,
        }))),
        ..ChromeModel::default()
    }
}

/// 一条被绘制的文本（左上角 + 宽高）。
#[derive(Debug, Clone)]
struct TextBox {
    rect: egui::Rect,
    text: String,
}

/// 跑一帧并把所有文本包围盒与设置页命中区收集出来。
fn run_model(model: ChromeModel) -> (Vec<TextBox>, lumino_ui_egui::settings::SettingsHits) {
    run_model_sized(model, WIDTH, HEIGHT)
}

/// 指定视口尺寸的版本（对话框场景用窄窗口，避免「整窗场景」把长文当单行）。
fn run_model_sized(
    model: ChromeModel,
    width: f32,
    height: f32,
) -> (Vec<TextBox>, lumino_ui_egui::settings::SettingsHits) {
    let ctx = egui::Context::default();
    let mut icons = IconCache::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, height),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        chrome::show_bands(ui, &model, &mut icons);
    });
    // 测试不接渲染后端：字体图集的增量必须显式丢弃，否则 epaint 会在析构时报错。
    output.textures_delta.clear();
    let mut boxes = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut boxes);
    }
    let hits = lumino_ui_egui::settings::hits(&ctx);
    (boxes, hits)
}

fn text_boxes(model: ChromeModel) -> Vec<TextBox> {
    run_model(model).0
}

fn collect(shape: &egui::Shape, out: &mut Vec<TextBox>) {
    match shape {
        egui::Shape::Text(text) => out.push(TextBox {
            rect: egui::Rect::from_min_size(text.pos, text.galley.size()),
            text: text.galley.text().to_owned(),
        }),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, out);
            }
        }
        _ => {}
    }
}

/// 卡片内的文本（不含菜单列、卡片外的确认按钮与标题栏）。
fn card_text_boxes(boxes: &[TextBox]) -> Vec<egui::Rect> {
    boxes
        .iter()
        .map(|b| b.rect)
        .filter(|r| {
            r.left() >= CARD_LEFT - 1.0
                && r.left() <= CARD_RIGHT
                && r.top() >= 36.0
                && r.bottom() <= CARD_BOTTOM
        })
        .collect()
}

fn assert_inside_card(boxes: &[TextBox]) {
    for rect in card_text_boxes(boxes) {
        assert!(
            rect.right() <= CARD_RIGHT + 0.5,
            "设置卡片文本越界：{rect:?}（卡片右缘 {CARD_RIGHT}）"
        );
    }
}

#[test]
fn settings_menu_label_is_truncated() {
    let page = UiPageModel {
        title: "界面".to_owned(),
        rows: Vec::new(),
    };
    let boxes = text_boxes(snapshot(SettingsPageModel::Ui(Box::new(page))));
    let menu: Vec<egui::Rect> = boxes
        .iter()
        .filter(|b| b.text != ">")
        .map(|b| b.rect)
        .filter(|r| r.left() >= 80.0 && r.left() <= MENU_TEXT_RIGHT + 4.0 && r.width() > 1.0)
        .collect();
    assert!(!menu.is_empty(), "应至少绘制出菜单文字");
    for rect in menu {
        assert!(
            rect.right() <= MENU_TEXT_RIGHT + 0.5,
            "菜单项不得压到箭头/卡片：{rect:?}（菜单文字右缘 {MENU_TEXT_RIGHT}）"
        );
    }
}

#[test]
fn settings_long_paragraph_wraps_inside_card() {
    let page = UiPageModel {
        title: "调色板".to_owned(),
        rows: vec![
            UiRow::Section {
                title: "这是一个特别长的分节标题，长到单行一定放不下，必须省略。\
                        再补一段让它远远超过卡片宽度，用来证明标题行也走省略号"
                    .to_owned(),
                hint: String::new(),
            },
            UiRow::Static {
                // 说明性长文本：必须折行，不能横穿卡片边框。
                text: "颜色预览：选中调色板后，下方会列出该调色板包含的全部颜色。\
                       如果当前工程已经加载 MIDI，调色板会被锁定，此时不能切换；\
                       请先关闭 MIDI 工程再回来修改。\
                       这段文字会继续变长，用来确保单行绝对放不下，必须折成多行显示。\
                       再补一句：颜色预览只影响界面配色，不会改动工程里的任何音符数据。"
                    .to_owned(),
            },
            UiRow::EchoCave {
                text: "这是一句很长的回声洞彩蛋文字，它只能在单行里显示，因此必须被省略".to_owned(),
                opacity: 1.0,
            },
            UiRow::Button {
                field: UiField::BrowseProgramFont,
                label: "一个特别长的按钮文案，长到按钮放不下时必须显示省略号".to_owned(),
            },
        ],
    };
    let boxes = text_boxes(snapshot(SettingsPageModel::Ui(Box::new(page))));
    for b in &boxes {
        println!("BOX {:?} {:?}", b.rect, b.text);
    }
    assert_inside_card(&boxes);

    // 说明段落必须真的折行（出现一个明显高于单行的文本块，且仍收在卡片内）。
    assert!(
        card_text_boxes(&boxes)
            .iter()
            .any(|rect| rect.height() >= 24.0 && rect.width() <= CARD_RIGHT - CARD_LEFT),
        "长说明文字必须折行：没有任何多行文本块"
    );
}

#[test]
fn settings_cloud_rows_are_truncated_before_buttons() {
    use lumino_ui_egui::chrome::{CloudConnModel, CloudPageModel};
    let page = CloudPageModel {
        title: "云管理".to_owned(),
        alert: None,
        add_label: "添加连接".to_owned(),
        empty_text: "尚未添加云存储连接。支持 FTP / SFTP / WebDAV，连接信息仅存储在本机。"
            .to_owned(),
        rows: vec![CloudConnModel {
            id: "conn-1".to_owned(),
            title: "这是一个特别长的连接名称，长到一定会压住右侧按钮 [sftp]，\
                    再补一段确保它超过一整行：家庭 NAS 备份盘（只读归档与素材库，\
                    夜间同步，白天只读）"
                .to_owned(),
            address: "sftp://a-very-long-host-name.example.com:2222/very/long/path/\
                      that/keeps/going/for/a/while"
                .to_owned(),
            status: "在线".to_owned(),
            online: true,
            connect_label: "断开".to_owned(),
            manage_label: "管理文件".to_owned(),
            delete_label: "删除".to_owned(),
        }],
    };
    let (boxes, hits) = run_model(snapshot(SettingsPageModel::Cloud(page)));
    assert_inside_card(&boxes);

    // 行文本必须停在操作按钮左侧：先把按钮标签自身排除，再比较左缘。
    let button_left = hits
        .cloud_buttons
        .iter()
        .filter(|(key, _)| key != "add")
        .map(|(_, rect)| rect.left())
        .fold(f32::INFINITY, f32::min);
    assert!(button_left.is_finite(), "云管理页应记录操作按钮矩形");
    for rect in card_text_boxes(&boxes) {
        if hits
            .cloud_buttons
            .iter()
            .any(|(_, button)| button.expand(0.5).contains_rect(rect))
        {
            continue;
        }
        assert!(
            rect.right() <= button_left - 4.0,
            "连接名称 / 地址不得压住操作按钮：{rect:?}（按钮左缘 {button_left}）"
        );
    }
}

#[test]
fn settings_audio_and_onion_pages_fit_card() {
    use lumino_ui_egui::chrome::{
        AudioControlModel, AudioField, AudioPageModel, AudioRowModel, OnionSkinPageModel,
    };

    let audio = AudioPageModel {
        title: "音频".to_owned(),
        rows: vec![
            AudioRowModel {
                field: AudioField::OutputType,
                label: "输出类型（这里故意写得很长，长到必须省略，不能压住右侧下拉框）".to_owned(),
                hint: String::new(),
                control: AudioControlModel::Combo {
                    options: vec!["Built-in".to_owned(), "KDMAPI".to_owned()],
                    selected: 0,
                },
                width: 200.0,
            },
            AudioRowModel {
                field: AudioField::AudioOutput,
                label: "输出设备".to_owned(),
                hint: String::new(),
                control: AudioControlModel::Static {
                    text:
                        "未找到输出设备：请检查系统音频服务是否运行，并在刷新设备列表后重新选择；\
                           如果仍然为空，请确认当前输出类型与后端匹配。\
                           这段状态说明继续变长，用来验证超宽时会被省略号截断，\
                           而不是越过卡片右缘。"
                            .to_owned(),
                    dim: true,
                },
                width: 200.0,
            },
        ],
    };
    let (boxes, hits) = run_model(snapshot(SettingsPageModel::Audio(audio)));
    assert_inside_card(&boxes);
    // 每个控件所在行的左侧文本必须停在控件左缘之前（不能压住下拉框 / 按钮）。
    for (_, control) in &hits.audio_rows {
        for rect in card_text_boxes(&boxes) {
            let same_row = rect.center().y >= control.top() && rect.center().y <= control.bottom();
            if same_row && rect.left() < control.left() {
                assert!(
                    rect.right() <= control.left() - 4.0,
                    "音频页标签不得压住右侧控件：{rect:?}（控件 {control:?}）"
                );
            }
        }
    }

    let onion = OnionSkinPageModel {
        title: "洋葱皮设置".to_owned(),
        label: "洋葱皮渲染方式（英文界面下这个标签会显著变长，必须截断）".to_owned(),
        options: vec!["GPU".to_owned(), "CPU".to_owned()],
        selected: 0,
        locked: true,
        locked_value: "GPU".to_owned(),
        locked_hint: "加载 MIDI 后不可切换，请先关闭 MIDI 工程；\
                      关闭前保存进度，避免丢失未写入的编辑。\
                      这条说明会继续变长，用来确保单行绝对放不下，必须折成多行显示。"
            .to_owned(),
        hint: "CPU 模式会禁用纵向卷帘、工程走带、瀑布流与视频导出；\
               如果只是临时排查渲染问题，建议切换到 GPU 后重新加载工程。\
               这段文字同样会继续变长，用来验证折行后仍然收在卡片内部。"
            .to_owned(),
    };
    let boxes = text_boxes(snapshot(SettingsPageModel::OnionSkin(onion)));
    assert_inside_card(&boxes);
    assert!(
        card_text_boxes(&boxes)
            .iter()
            .any(|rect| rect.height() >= 24.0),
        "洋葱皮页的长说明必须折行"
    );
}

#[test]
fn save_confirm_long_message_wraps_and_stays_inside_window() {
    let model = ChromeModel {
        dialog: Some(DialogModel::SaveConfirm(SaveConfirmModel {
            titlebar: false,
            palette: DialogPalette::DARK,
            message: "当前工程包含大量未保存的修改，关闭窗口前是否保留这些更改？\
                      选择「关闭」将丢弃全部未保存内容。"
                .to_owned(),
            save: "保存".to_owned(),
            close: "关闭".to_owned(),
            cancel: "取消".to_owned(),
        })),
        ..ChromeModel::default()
    };
    let (boxes, _) = run_model_sized(model, 560.0, 260.0);
    let message = boxes
        .iter()
        .map(|b| b.rect)
        .find(|r| r.height() > 24.0)
        .expect("长提示必须折行（应出现高于单行的文本块）");
    assert!(
        message.left() >= 16.0 && message.right() <= 544.0,
        "提示文本越出窗口内边距：{message:?}"
    );
    assert!(
        message.bottom() <= 126.0,
        "折行后的提示不得压到按钮行：{message:?}"
    );
}
