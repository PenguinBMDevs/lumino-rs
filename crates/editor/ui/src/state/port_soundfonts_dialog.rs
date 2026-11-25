//! 端口音色库对话框状态（REND-005 P4）。
//!
//! UI v1 每个端口只显示/编辑**一个**音色库；模型层仍保留 `Vec<String>`
//! 以对齐引擎的优先级回退链。未改动的多路径条目原样保留，避免打开对话框
//! 再点确定就静默丢掉回退库。

use lumino_midi_model::multi_port::{PortSoundfontMap, effective_port};

/// 对话框中的一行（一个 MIDI 端口）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSoundfontRow {
    /// MIDI 端口号（0 = 全局默认，可覆盖）。
    pub port: u8,
    /// 该端口配置的音色库路径链；空 = 未配置（回退全局）。
    pub paths: Vec<String>,
}

/// 端口音色库对话框状态。
#[derive(Debug, Clone, Default)]
pub struct PortSoundfontsDialogState {
    rows: Vec<PortSoundfontRow>,
}

impl PortSoundfontsDialogState {
    /// 创建空状态（对话框 Root 构造时使用）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 用工程映射 + 当前文档端口数初始化行。
    ///
    /// 行范围覆盖 `0..=max(文档 max_port, 已配置的最高端口)`：
    /// - 文档用不到的端口不展示；
    /// - 已配置但当前布局缩小的端口仍保留，避免「打开对话框再确定」丢配置。
    pub fn open(&mut self, map: &PortSoundfontMap, document_max_port: u8) {
        let configured_max = map.entries().last().map_or(0, |entry| entry.port);
        let max_port = effective_port(document_max_port.max(configured_max));
        self.rows = (0..=max_port)
            .map(|port| PortSoundfontRow {
                port,
                paths: map
                    .get(port)
                    .map(|entry| entry.paths.clone())
                    .unwrap_or_default(),
            })
            .collect();
    }

    /// 当前行（按端口升序）。
    pub fn rows(&self) -> &[PortSoundfontRow] {
        &self.rows
    }

    /// 某端口当前显示的首个路径（`None` = 未配置）。
    pub fn first_path(&self, port: u8) -> Option<&str> {
        self.row(port)?
            .paths
            .first()
            .map(std::string::String::as_str)
    }

    /// 设置某端口的音色库（v1：替换为单库）。
    pub fn set_path(&mut self, port: u8, path: String) {
        if let Some(row) = self.row_mut(port) {
            row.paths = vec![path];
        }
    }

    /// 清除某端口的覆盖（回退全局）。
    pub fn clear(&mut self, port: u8) {
        if let Some(row) = self.row_mut(port) {
            row.paths.clear();
        }
    }

    /// 构造最终映射（空路径由 `PortSoundfontMap::set` 自动移除）。
    pub fn to_map(&self) -> PortSoundfontMap {
        let mut map = PortSoundfontMap::default();
        for row in &self.rows {
            map.set(row.port, row.paths.clone());
        }
        map
    }

    fn row(&self, port: u8) -> Option<&PortSoundfontRow> {
        self.rows.iter().find(|row| row.port == port)
    }

    fn row_mut(&mut self, port: u8) -> Option<&mut PortSoundfontRow> {
        self.rows.iter_mut().find(|row| row.port == port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::multi_port::PortSoundfontEntry;

    fn map_with(entries: &[(u8, &[&str])]) -> PortSoundfontMap {
        PortSoundfontMap::from_entries(
            entries
                .iter()
                .map(|(port, paths)| {
                    PortSoundfontEntry::new(*port, paths.iter().map(|path| (*path).to_string()))
                })
                .collect::<Vec<_>>(),
        )
    }

    /// 单端口文档 + 空映射：只展示端口 0。
    #[test]
    fn empty_map_single_port_shows_port_zero_only() {
        let mut state = PortSoundfontsDialogState::new();
        state.open(&PortSoundfontMap::default(), 0);
        assert_eq!(state.rows().len(), 1);
        assert_eq!(state.rows()[0].port, 0);
        assert!(state.rows()[0].paths.is_empty());
    }

    /// 多端口文档：展示 0..=max_port，未配置端口为空。
    #[test]
    fn document_max_port_controls_row_range() {
        let mut state = PortSoundfontsDialogState::new();
        state.open(&PortSoundfontMap::default(), 2);
        let ports: Vec<u8> = state.rows().iter().map(|row| row.port).collect();
        assert_eq!(ports, vec![0, 1, 2]);
    }

    /// 已配置端口高于当前文档布局：仍展示，避免打开再确定静默丢配置。
    #[test]
    fn configured_ports_beyond_layout_are_kept() {
        let map = map_with(&[(5, &["five.sf2"])]);
        let mut state = PortSoundfontsDialogState::new();
        state.open(&map, 1);
        let ports: Vec<u8> = state.rows().iter().map(|row| row.port).collect();
        assert_eq!(ports, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(state.first_path(5), Some("five.sf2"));
    }

    /// 端口 0 可覆盖全局。
    #[test]
    fn port_zero_override_roundtrips() {
        let map = map_with(&[(0, &["zero.sf2"]), (2, &["two.sf2"])]);
        let mut state = PortSoundfontsDialogState::new();
        state.open(&map, 2);
        assert_eq!(state.to_map(), map);
    }

    /// 未改动的多路径回退链必须原样保留。
    #[test]
    fn untouched_multi_path_entries_are_preserved() {
        let map = map_with(&[(1, &["a.sf2", "b.sf2"])]);
        let mut state = PortSoundfontsDialogState::new();
        state.open(&map, 1);
        assert_eq!(state.to_map(), map, "未编辑的行不得丢失回退链");
    }

    /// 编辑某端口会替换为单库；清除后回退全局。
    #[test]
    fn edit_replaces_chain_and_clear_removes_override() {
        let mut state = PortSoundfontsDialogState::new();
        state.open(&PortSoundfontMap::default(), 3);
        state.set_path(2, "new.sf2".to_string());
        assert_eq!(state.first_path(2), Some("new.sf2"));

        let map = state.to_map();
        assert_eq!(
            map.get(2).map(|entry| entry.paths.clone()),
            Some(vec!["new.sf2".to_string()])
        );

        state.clear(2);
        assert_eq!(state.first_path(2), None);
        assert!(state.to_map().is_empty());
    }
}
