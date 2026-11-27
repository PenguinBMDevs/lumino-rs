//! Root 侧工程级「端口 → 音色库」映射状态测试（REND-005）。

use crate::root::Root;
use lumino_core::storage::config::UiConfig;
use lumino_midi_model::multi_port::PortSoundfontMap;

/// 修改映射必须置工程脏；关闭/新建工程必须清空映射。
#[test]
fn test_port_soundfonts_set_marks_dirty_and_reset_clears() {
    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    assert!(
        root.port_soundfonts().is_empty(),
        "新工程默认不得有端口音色库覆盖"
    );

    let mut map = PortSoundfontMap::default();
    map.set(1, vec!["D:/音色库/piano.sf2".to_string()]);
    root.set_port_soundfonts(map.clone());

    assert_eq!(root.port_soundfonts(), &map);
    assert!(
        root.editor.editor_state.data.modified,
        "修改端口音色库映射必须标记工程未保存（否则 Ctrl+S/关闭确认会漏掉）"
    );

    root.reset_project_settings();
    assert!(
        root.port_soundfonts().is_empty(),
        "关闭/新建工程必须清空端口音色库映射"
    );
}
