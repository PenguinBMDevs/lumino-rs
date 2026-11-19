//! 端口 → 音色库覆盖映射（REND-005 基础层）。
//!
//! 多端口文档中，每个端口可以绑定自己的 SoundFont；未配置的端口回退到
//! 全局音色库。本模块只提供**纯数据**与解析规则，不依赖 xsynth / 文件系统，
//! 导出、实时、工程与 UI 共用同一套语义。
//!
//! 语义要点：
//! - 端口号按 [`effective_port`] 折叠到产品上限（16 端口）；
//! - 同一端口重复配置以**后出现者**为准；
//! - `paths` 为空视为“未配置”，解析时回退全局；
//! - 路径去空、去重（保持首次出现顺序）；
//! - 引擎侧 `SetSoundfonts(Vec)` 是**优先级回退链**，不是叠加混音；
//!   v1 UI 每个端口只写一条，模型保留 `Vec` 以对齐引擎与后续扩展。

use serde::{Deserialize, Serialize};

use super::effective_port;

/// 单个端口的音色库覆盖配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortSoundfontEntry {
    /// MIDI 端口号（保存原始 u7；消费/归一化时经 [`effective_port`] 折叠）。
    pub port: u8,
    /// 该端口使用的音色库路径列表。
    ///
    /// v1 UI 只写 0 或 1 条；引擎按 `Vec` 消费（优先级回退链）。
    /// 空列表 = 未配置（回退全局）。
    #[serde(default)]
    pub paths: Vec<String>,
}

impl PortSoundfontEntry {
    /// 构造一条端口覆盖配置（不做归一化；归一化由 [`PortSoundfontMap`] 负责）。
    pub fn new(port: u8, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            port,
            paths: paths.into_iter().map(Into::into).collect(),
        }
    }
}

/// `port → 音色库路径` 映射。
///
/// 内部按端口升序存储、端口唯一、路径已去空去重；构造与修改入口都会做归一化。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortSoundfontMap {
    entries: Vec<PortSoundfontEntry>,
}

impl PortSoundfontMap {
    /// 从工程/消息中的条目列表构造（任意顺序，自动归一化）。
    pub fn from_entries(entries: impl IntoIterator<Item = PortSoundfontEntry>) -> Self {
        let mut map = Self::default();
        for entry in entries {
            map.set(entry.port, entry.paths);
        }
        map
    }

    /// 设置某端口的覆盖库；`paths` 为空等价于移除该端口覆盖。
    ///
    /// 端口按 [`effective_port`] 折叠；同端口重复设置以最后一次为准。
    pub fn set(&mut self, port: u8, paths: Vec<String>) {
        let port = effective_port(port);
        let paths = normalize_paths(paths);
        if paths.is_empty() {
            self.remove(port);
            return;
        }
        match self.entries.binary_search_by_key(&port, |entry| entry.port) {
            Ok(index) => self.entries[index].paths = paths,
            Err(index) => self
                .entries
                .insert(index, PortSoundfontEntry { port, paths }),
        }
    }

    /// 移除某端口的覆盖（端口同样按 [`effective_port`] 折叠）。
    pub fn remove(&mut self, port: u8) {
        let port = effective_port(port);
        self.entries.retain(|entry| entry.port != port);
    }

    /// 获取某端口的覆盖（未配置返回 `None`）。
    pub fn get(&self, port: u8) -> Option<&PortSoundfontEntry> {
        let port = effective_port(port);
        self.entries
            .binary_search_by_key(&port, |entry| entry.port)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// 解析某端口最终使用的音色库路径：有覆盖用覆盖，否则用全局。
    pub fn resolve<'a>(&'a self, port: u8, global: &'a [String]) -> &'a [String] {
        self.get(port)
            .map_or(global, |entry| entry.paths.as_slice())
    }

    /// 端口 `0..=max_port` 范围内的覆盖条目（按端口升序）。
    ///
    /// 超出产品上限的 `max_port` 同样按 [`effective_port`] 折叠。
    pub fn entries_for_max_port(&self, max_port: u8) -> impl Iterator<Item = &PortSoundfontEntry> {
        let max_port = effective_port(max_port);
        self.entries
            .iter()
            .filter(move |entry| entry.port <= max_port)
    }

    /// 是否为空的映射（无任何端口覆盖）。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 覆盖条目数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 按端口升序返回全部覆盖条目。
    pub fn entries(&self) -> &[PortSoundfontEntry] {
        &self.entries
    }
}

/// 归一化路径列表：去掉空/纯空白项，按首次出现顺序去重。
fn normalize_paths(paths: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(paths.len());
    for path in paths {
        if path.trim().is_empty() {
            continue;
        }
        if normalized.iter().any(|existing| existing == &path) {
            continue;
        }
        normalized.push(path);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn global() -> Vec<String> {
        paths(&["global.sf2"])
    }

    #[test]
    fn empty_map_falls_back_to_global() {
        let map = PortSoundfontMap::default();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        assert_eq!(map.resolve(0, &global()), global());
        assert_eq!(map.resolve(7, &global()), global());
    }

    #[test]
    fn configured_port_uses_override_and_others_fall_back() {
        let mut map = PortSoundfontMap::default();
        map.set(1, paths(&["port1.sf2"]));

        assert_eq!(map.resolve(1, &global()), paths(&["port1.sf2"]));
        assert_eq!(map.resolve(0, &global()), global());
        assert_eq!(map.resolve(2, &global()), global());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn port_zero_can_be_overridden() {
        let mut map = PortSoundfontMap::default();
        map.set(0, paths(&["port0.sf2"]));
        assert_eq!(map.resolve(0, &global()), paths(&["port0.sf2"]));
    }

    #[test]
    fn duplicate_ports_last_wins_after_clamping() {
        let map = PortSoundfontMap::from_entries(vec![
            PortSoundfontEntry::new(15, ["a.sf2"]),
            PortSoundfontEntry::new(16, ["b.sf2"]), // 折叠到端口 15
        ]);
        assert_eq!(map.len(), 1, "端口 15/16 归一化后应合并");
        assert_eq!(map.resolve(15, &global()), paths(&["b.sf2"]));
    }

    #[test]
    fn empty_paths_remove_override() {
        let mut map = PortSoundfontMap::default();
        map.set(1, paths(&["port1.sf2"]));
        map.set(1, Vec::new());
        assert!(map.is_empty());
        assert_eq!(map.resolve(1, &global()), global());
    }

    #[test]
    fn paths_drop_empty_and_dedupe() {
        let mut map = PortSoundfontMap::default();
        map.set(1, paths(&["a.sf2", "", "  ", "a.sf2", "b.sf2", "b.sf2"]));
        assert_eq!(map.resolve(1, &global()), paths(&["a.sf2", "b.sf2"]));
    }

    #[test]
    fn entries_are_sorted_by_port() {
        let map = PortSoundfontMap::from_entries(vec![
            PortSoundfontEntry::new(3, ["c.sf2"]),
            PortSoundfontEntry::new(1, ["a.sf2"]),
            PortSoundfontEntry::new(2, ["b.sf2"]),
        ]);
        let ports: Vec<u8> = map.entries().iter().map(|entry| entry.port).collect();
        assert_eq!(ports, vec![1, 2, 3]);
    }

    #[test]
    fn entries_for_max_port_filters_and_clamps() {
        let map = PortSoundfontMap::from_entries(vec![
            PortSoundfontEntry::new(0, ["p0.sf2"]),
            PortSoundfontEntry::new(1, ["p1.sf2"]),
            PortSoundfontEntry::new(5, ["p5.sf2"]),
        ]);

        let ports = |max_port: u8| -> Vec<u8> {
            map.entries_for_max_port(max_port)
                .map(|entry| entry.port)
                .collect()
        };

        assert_eq!(ports(0), vec![0]);
        assert_eq!(ports(1), vec![0, 1]);
        assert_eq!(ports(5), vec![0, 1, 5]);
        // 超上限 max_port 折叠到端口 15，仍返回全部（产品上限内）条目。
        assert_eq!(ports(127), vec![0, 1, 5]);
    }

    #[test]
    fn entry_list_serde_roundtrip() {
        let entries = vec![
            PortSoundfontEntry::new(0, ["p0.sf2"]),
            PortSoundfontEntry::new(2, ["p2a.sf2", "p2b.sf2"]),
        ];
        let json = serde_json::to_string(&entries).expect("序列化端口音色库条目失败");
        let decoded: Vec<PortSoundfontEntry> =
            serde_json::from_str(&json).expect("反序列化端口音色库条目失败");
        assert_eq!(decoded, entries);
    }

    #[test]
    fn missing_paths_field_defaults_empty() {
        let entry: PortSoundfontEntry =
            serde_json::from_str(r#"{"port":2}"#).expect("缺 paths 字段应可解析");
        assert_eq!(entry.port, 2);
        assert!(entry.paths.is_empty());
    }
}
