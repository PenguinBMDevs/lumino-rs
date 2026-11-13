use lumino_extras::i18n::{Language, main_translations};

use crate::event::Event;

/// 菜单类型
#[derive(Debug, Clone)]
pub enum MenuKind {
    /// 文件菜单
    File,
    /// 编辑菜单
    Edit,
    /// 视图菜单
    View,
    /// 帮助菜单
    Help,
}

impl MenuKind {
    /// 获取菜单类型的显示名称。
    ///
    /// ⚠️ 刻意**没有** `impl Display`：语言相关的显示名必须显式传 `lang`，否则会退化
    /// 成「硬编码某一种语言」（UI-018 缺陷 D-2：英文界面下 macOS 菜单栏永远中文）。
    /// 旧版 菜单栏（`menu_bar`）与 macOS 原生菜单都走本方法。
    pub fn display_name(&self, lang: Language) -> &'static str {
        let translations = main_translations(lang);
        match self {
            Self::File => translations.menu_file,
            Self::Edit => translations.menu_edit,
            Self::View => translations.menu_view,
            Self::Help => translations.menu_help,
        }
    }
}

/// 菜单项类型
#[derive(Debug, Clone)]
pub enum MenuItem {
    // 用于 i18n 的 Action(Event, Fn) 或类似结构
    /// 可点击的动作菜单项
    Action(Event),
    /// 置灰禁用的菜单项（保留事件用于显示名，但不可点击）
    ActionDisabled(Event),
    /// 分隔线
    Separator,
    // 子菜单(Vec<MenuItem>, Fn)
    /// 子菜单（子项列表与标题）
    Submenu(Vec<MenuItem>, String),
}

/// 菜单配置（类型 + 菜单项列表）
#[derive(Debug, Clone)]
pub struct MenuConfig {
    /// 菜单类型
    pub kind: MenuKind,
    /// 菜单项列表
    pub items: Vec<MenuItem>,
}

/// 构建文件菜单配置
pub fn file_menu(lang: Language, export_material_enabled: bool) -> MenuConfig {
    let translations = main_translations(lang);
    use crate::event::menu::file;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::File,
        items: vec![
            Action(crate::event::Event::menu_file(file::Event::new_file())),
            Action(crate::event::Event::menu_file(file::Event::open())),
            Action(crate::event::Event::menu_file(file::Event::save())),
            Action(crate::event::Event::menu_file(file::Event::close())),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::import_files())),
            // 云存储导入/保存（无连接时由 runner 弹出连接面板引导）
            Action(crate::event::Event::menu_file(
                file::Event::import_from_cloud(),
            )),
            Action(crate::event::Event::menu_file(file::Event::save_to_cloud())),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::export_midi())),
            Submenu(
                vec![
                    Action(crate::event::Event::menu_file(
                        file::Event::export_project_archive(),
                    )),
                    Action(crate::event::Event::menu_file(
                        file::Event::export_project_folder(),
                    )),
                ],
                translations.file_export_project.to_string(),
            ),
            // 导出为素材：仅在存在音符框选时可用（走带视图跨音轨框选 / 卷帘选中音符）
            if export_material_enabled {
                Action(crate::event::Event::menu_file(
                    file::Event::export_material(),
                ))
            } else {
                ActionDisabled(crate::event::Event::menu_file(
                    file::Event::export_material(),
                ))
            },
            Separator,
            Action(crate::event::Event::menu_file(
                file::Event::project_settings(),
            )),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::settings())),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::exit())),
        ],
    }
}

/// 构建编辑菜单配置
pub fn edit_menu(_lang: Language) -> MenuConfig {
    use crate::event::menu::edit;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Edit,
        items: vec![
            Action(crate::event::Event::menu_edit(edit::Event::undo())),
            Action(crate::event::Event::menu_edit(edit::Event::redo())),
            Separator,
            Action(crate::event::Event::menu_edit(edit::Event::cut())),
            Action(crate::event::Event::menu_edit(edit::Event::copy())),
            Action(crate::event::Event::menu_edit(edit::Event::paste())),
            Action(crate::event::Event::menu_edit(edit::Event::select_all())),
            Separator,
            Action(crate::event::Event::menu_edit(edit::Event::find())),
        ],
    }
}

/// 构建视图菜单配置
pub fn view_menu(_lang: Language) -> MenuConfig {
    use crate::event::menu::view;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::View,
        items: vec![
            Action(crate::event::Event::menu_view(view::Event::zoom_in())),
            Action(crate::event::Event::menu_view(view::Event::zoom_out())),
            Action(crate::event::Event::menu_view(view::Event::zoom_reset())),
        ],
    }
}

/// 构建帮助菜单配置
pub fn help_menu(_lang: Language) -> MenuConfig {
    use crate::event::menu::help;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Help,
        items: vec![Action(crate::event::Event::menu_help(help::Event::about()))],
    }
}

/// 构建全部四个菜单（文件/编辑/视图/帮助）的配置数组
pub fn menus(lang: Language, export_material_enabled: bool) -> [MenuConfig; 4] {
    [
        file_menu(lang, export_material_enabled),
        edit_menu(lang),
        view_menu(lang),
        help_menu(lang),
    ]
}

/// 获取事件的友好显示名称
pub fn event_display_name(event: &Event, lang: Language) -> String {
    let translations = main_translations(lang);
    use crate::event::menu::{
        edit::Event as EditEvent, file::Event as FileEvent, help::Event as HelpEvent,
        view::Event as ViewEvent,
    };

    match event {
        Event::Menu(menu_event) => match menu_event {
            crate::event::menu::Event::File(file_event) => match file_event {
                FileEvent::New => translations.file_new.to_string(),
                FileEvent::Open => translations.file_open.to_string(),
                FileEvent::Save => translations.file_save.to_string(),
                FileEvent::Close => translations.file_close.to_string(),
                FileEvent::ImportFiles => translations.file_import.to_string(),
                FileEvent::ImportFromCloud => translations.file_import_from_cloud.to_string(),
                FileEvent::SaveToCloud => translations.file_save_to_cloud.to_string(),
                FileEvent::ExportProjectArchive => translations.file_export_archive.to_string(),
                FileEvent::ExportProjectFolder => translations.file_export_folder.to_string(),
                FileEvent::ExportMaterial => translations.file_export_material.to_string(),
                FileEvent::ExportMidi => translations.file_export_midi.to_string(),
                FileEvent::ProjectSettings => translations.file_project_settings.to_string(),
                FileEvent::Settings => translations.file_settings.to_string(),
                FileEvent::Exit => translations.file_exit.to_string(),
                _ => format!("{file_event:?}"),
            },
            crate::event::menu::Event::Edit(edit_event) => match edit_event {
                EditEvent::Undo => translations.edit_undo.to_string(),
                EditEvent::Redo => translations.edit_redo.to_string(),
                EditEvent::Cut => translations.edit_cut.to_string(),
                EditEvent::Copy => translations.edit_copy.to_string(),
                EditEvent::Paste => translations.edit_paste.to_string(),
                EditEvent::SelectAll => translations.edit_select_all.to_string(),
                EditEvent::Find => translations.edit_find.to_string(),
            },
            crate::event::menu::Event::View(view_event) => match view_event {
                ViewEvent::ZoomIn => translations.view_zoom_in.to_string(),
                ViewEvent::ZoomOut => translations.view_zoom_out.to_string(),
                ViewEvent::ZoomReset => translations.view_zoom_reset.to_string(),
                _ => format!("{view_event:?}"),
            },
            crate::event::menu::Event::Help(help_event) => match help_event {
                HelpEvent::About => translations.help_about.to_string(),
            },
        },
        Event::Window(window_event) => format!("{window_event:?}"),
        Event::Cloud(cloud_event) => format!("{cloud_event:?}"),
    }
}
