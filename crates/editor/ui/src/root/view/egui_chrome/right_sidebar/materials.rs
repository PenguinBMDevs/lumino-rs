//! 右侧栏「素材库」模型镜像与动作翻译。
//!
//! 列表分区 / 右键菜单目标 / 重命名缓冲 / 添加下拉全部来自生产
//! `RightSidebar.materials`；悬停提示行与所有文案在这里按当前语言预计算。
//! 素材拖出只在按下瞬间回传 `MaterialDragStarted(index)`，跟随预览与松手
//! 放置继续由 旧版 编辑器状态机驱动。

use lumino_extras::i18n::main_translations;
use lumino_message::MaterialContextMenuItem as SidebarContextItem;
use lumino_message::RightSidebarAction as SidebarAction;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction as PanelAction;
use lumino_ui_egui::chrome::{
    MaterialContextLabels, MaterialContextMenuItem, MaterialContextMenuModel, MaterialItemModel,
    MaterialMenuEntryModel, MaterialMenuKind, MaterialSectionModel, MaterialsPanelModel,
};

use crate::Message;
use crate::right_sidebar::material::{MaterialEntry, MaterialSource};
use crate::root::Root;

/// 构造素材库面板模型。
pub(super) fn model(root: &Root) -> MaterialsPanelModel {
    let language = root.settings.display.language;
    let t = main_translations(language);
    let materials = &root.right_sidebar.materials;

    let item = |entry: &MaterialEntry, index: usize| MaterialItemModel {
        index,
        name: entry.name.clone(),
        valid: entry.valid,
        renaming: materials
            .renaming_material
            .as_ref()
            .filter(|(target, _)| *target == index)
            .map(|(_, buffer)| buffer.clone()),
        tooltip: tooltip_lines(entry, t),
    };
    let section = |source: MaterialSource, label: &str| MaterialSectionModel {
        label: label.to_owned(),
        entries: materials
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.source == source)
            .map(|(index, entry)| item(entry, index))
            .collect(),
    };

    let mut sections = Vec::new();
    let builtin = section(MaterialSource::BuiltIn, t.material_section_builtin);
    if !builtin.entries.is_empty() {
        sections.push(builtin);
    }
    let user = section(MaterialSource::User, t.material_section_user);
    if !user.entries.is_empty() {
        sections.push(user);
    }

    let placeholder = if !materials.is_initialized() {
        Some(t.material_invalid.to_owned())
    } else if materials.entries.is_empty() {
        Some(t.material_section_user.to_owned())
    } else {
        None
    };

    let add_menu = materials.add_menu_open.then(|| {
        vec![
            MaterialMenuEntryModel {
                label: t.material_download_web.to_owned(),
                kind: MaterialMenuKind::DownloadFromWeb,
            },
            MaterialMenuEntryModel {
                label: t.material_import_local.to_owned(),
                kind: MaterialMenuKind::ImportFromLocal,
            },
        ]
    });

    let context_menu = materials.context_menu_target.and_then(|index| {
        let entry = materials.entries.get(index)?;
        let (x, y) = materials.context_menu_pos.unwrap_or((0.0, 0.0));
        Some(MaterialContextMenuModel {
            index,
            can_edit: entry.source == MaterialSource::User,
            x,
            y,
        })
    });

    MaterialsPanelModel {
        title: t.material_library.to_owned(),
        add_label: format!("+ {}", t.material_add),
        add_menu,
        placeholder,
        rename_placeholder: lumino_core::tr!("素材名称").to_string(),
        sections,
        context_labels: MaterialContextLabels {
            rename: lumino_core::tr!("重命名").to_string(),
            delete: lumino_core::tr!("删除").to_string(),
            upload: lumino_core::tr!("上传到云").to_string(),
        },
        context_menu,
    }
}

/// 悬停提示行：名称 / 作者 / 位置 / 轨道数 / 来源；无效素材只显示「素材无效」。
fn tooltip_lines(
    entry: &MaterialEntry,
    t: &'static lumino_extras::i18n::MainTranslations,
) -> Vec<String> {
    if !entry.valid {
        return vec![t.material_invalid.to_owned()];
    }
    let mut lines = vec![format!("{}{}", t.material_name_label, entry.name)];
    if !entry.author.is_empty() {
        lines.push(format!("{}{}", t.material_author_label, entry.author));
    }
    if let Some(path) = &entry.path {
        lines.push(format!("{}{}", t.material_location_label, path.display()));
    }
    if entry.track_count > 0 {
        lines.push(format!("{}{}", t.material_track_label, entry.track_count));
    }
    let source = match entry.source {
        MaterialSource::BuiltIn => t.material_section_builtin,
        MaterialSource::User => t.material_section_user,
    };
    lines.push(format!("{}{}", t.material_source_label, source));
    lines
}

/// 素材库动作 → 生产 `Message`（非素材动作返回 `None`）。
pub(super) fn action_message(action: &PanelAction) -> Option<Message> {
    Some(match action {
        PanelAction::MaterialAdd => Message::RightSidebar(SidebarAction::MaterialAddClicked),
        PanelAction::MaterialMenu(kind) => Message::RightSidebar(match kind {
            MaterialMenuKind::DownloadFromWeb => SidebarAction::MaterialDownloadFromWeb,
            MaterialMenuKind::ImportFromLocal => SidebarAction::MaterialImportFromLocal,
        }),
        PanelAction::MaterialDragStarted(index) => {
            Message::RightSidebar(SidebarAction::MaterialDragStarted(*index))
        }
        PanelAction::MaterialContextMenuOpened { index, x, y } => Message::Batch(vec![
            // 先同步面板局部光标位置，再打开菜单（生产在该动作里快照位置）。
            Message::RightSidebar(SidebarAction::MaterialCursorMoved(*x, *y)),
            Message::RightSidebar(SidebarAction::MaterialContextMenuOpened(*index)),
        ]),
        PanelAction::MaterialContextMenuClosed => {
            Message::RightSidebar(SidebarAction::MaterialContextMenuClosed)
        }
        PanelAction::MaterialContextMenuItemClicked { index, item } => Message::RightSidebar(
            SidebarAction::MaterialContextMenuItemClicked(*index, context_item(*item)),
        ),
        PanelAction::MaterialRenameInput(text) => {
            Message::RightSidebar(SidebarAction::MaterialRenameInputChanged(text.clone()))
        }
        PanelAction::MaterialRenameConfirmed => {
            Message::RightSidebar(SidebarAction::MaterialRenameConfirmed)
        }
        _ => return None,
    })
}

fn context_item(item: MaterialContextMenuItem) -> SidebarContextItem {
    match item {
        MaterialContextMenuItem::Rename => SidebarContextItem::Rename,
        MaterialContextMenuItem::Delete => SidebarContextItem::Delete,
        MaterialContextMenuItem::UploadToCloud => SidebarContextItem::UploadToCloud,
    }
}
