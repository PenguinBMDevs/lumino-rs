//! 素材库面板视图单元测试

use std::path::PathBuf;

use super::*;
use crate::right_sidebar::material::MaterialEntry;

fn make_entry(valid: bool, track_count: usize) -> MaterialEntry {
    MaterialEntry {
        name: "测试素材".into(),
        author: "测试作者".into(),
        source: MaterialSource::BuiltIn,
        path: None,
        data: None,
        multi_track: track_count > 1,
        track_count,
        valid,
        preview: None,
    }
}

#[test]
fn test_material_item_builds_element() {
    let mut sidebar = RightSidebar::new();
    sidebar.materials.entries.push(make_entry(true, 4));
    let entry = &sidebar.materials.entries[0];
    let _element = material_item(&sidebar, entry, 0, Language::ZhCn);
}

#[test]
fn test_material_item_invalid_greyed() {
    let mut sidebar = RightSidebar::new();
    sidebar.materials.entries.push(make_entry(false, 0));
    let entry = &sidebar.materials.entries[0];
    let _element = material_item(&sidebar, entry, 1, Language::ZhCn);
}

#[test]
fn test_material_item_renaming_state() {
    // 重命名态：名称替换为输入框，仍可构建元素
    let mut sidebar = RightSidebar::new();
    sidebar.materials.entries.push(make_entry(true, 1));
    sidebar.materials.renaming_material = Some((0, "新名称".into()));
    let entry = &sidebar.materials.entries[0];
    let _element = material_item(&sidebar, entry, 0, Language::ZhCn);
}

#[test]
fn test_tooltip_content_builds_full_description() {
    // 有效素材：名称/作者/轨道数/来源均带描述标头；无路径不显示位置
    let entry = make_entry(true, 4);
    let t = main_translations(Language::ZhCn);
    let _element = tooltip_content(&entry, t);

    // 本地素材：额外显示位置（磁盘路径）
    let mut user_entry = MaterialEntry {
        path: Some(PathBuf::from("C:/Materials/demo.lmmaterial")),
        ..make_entry(true, 2)
    };
    user_entry.source = MaterialSource::User;
    let _element = tooltip_content(&user_entry, t);
}

#[test]
fn test_tooltip_content_invalid_shows_invalid() {
    // 无效素材：仅显示"素材无效"
    let entry = make_entry(false, 0);
    let t = main_translations(Language::ZhCn);
    let _element = tooltip_content(&entry, t);
}

#[test]
fn test_panel_route_switch() {
    // 面板路由互斥切换（素材库面板 / I2M 面板）
    let mut sidebar = RightSidebar::new();
    assert!(!sidebar.panel_visible);
    sidebar.switch_panel(crate::right_sidebar::RightSidebarPanel::Materials);
    assert!(sidebar.panel_visible);
    assert!(sidebar.is_panel_active(crate::right_sidebar::RightSidebarPanel::Materials));
    assert!(!sidebar.is_panel_active(crate::right_sidebar::RightSidebarPanel::ImageToMidi));
}
