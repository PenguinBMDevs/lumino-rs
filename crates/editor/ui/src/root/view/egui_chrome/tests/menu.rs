//! 标题栏菜单测试：可用性闸门与动作解析。
//!
//! 从 `tests.rs` 拆出（后者已承载整窗外壳装配，长度接近上限）。

use lumino_core::i18n::Language;
use lumino_ui_egui::chrome::MenuEntry;

use crate::Message;
use crate::root::Root;

/// 跨轨框选（≥2 轨）时「导出为素材」必须可用：闸门、菜单项与动作解析三者同源。
///
/// 真机缺陷（2026-11-20）：跨轨选区导出后产物缺轨/为空，用户表现为
/// 「菜单不可用或点击没有产物」。闸门侧必须以**实际命中音符**为准，
/// 且走带跨轨选区要能完整进入 `active_selection`。
#[test]
fn titlebar_export_material_enabled_for_cross_track_selection() {
    use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};

    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    let doc = MidiDocument {
        notes: vec![
            ChunkedList::from_sorted(vec![NoteEvent::new(100, 300, 60, 100, 0)]),
            ChunkedList::from_sorted(vec![NoteEvent::new(150, 350, 64, 90, 1)]),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 480,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(2),
    };
    root.set_midi_document(doc);
    root.sidebar.route = crate::sidebar::Route::Arrangement;
    root.editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 480, 0, 127, 0, 1);

    assert!(
        root.export_material_enabled(),
        "跨轨框选命中音符时「导出为素材」不得置灰"
    );
    assert_eq!(
        root.active_selection().track_count(),
        2,
        "选区快照必须包含两条音轨（否则导出只会带一轨）"
    );

    let model = root.egui_chrome_model();
    let entries = &model
        .titlebar
        .expect("自绘标题栏应有模型")
        .menu
        .expect("应带可交互菜单树")
        .menus[0]
        .entries;
    let translations = lumino_core::i18n::main_translations(Language::ZhCn);
    let (enabled, action) = entries
        .iter()
        .find_map(|entry| match entry {
            MenuEntry::Action {
                label,
                enabled,
                action,
            } if *label == translations.file_export_material => Some((*enabled, *action)),
            _ => None,
        })
        .expect("文件菜单应包含「导出为素材」");
    assert!(enabled, "跨轨框选时菜单项不得置灰");
    assert!(
        matches!(
            root.menu_action_message(action),
            Some(Message::Core(crate::event::Event::Menu(
                crate::event::menu::Event::File(crate::event::menu::file::Event::ExportMaterial)
            )))
        ),
        "菜单动作必须解析回 ExportMaterial"
    );
}
