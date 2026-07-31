//! 音符合并分组逻辑
//!
//! 将 `EditorData::glue_selected_notes` 中的纯分组算法提取到此处，
//! 便于独立测试和后续复用。

/// 用于分组的音符元组：
/// `(原始索引, tick, key, length, velocity, channel)`
pub type NoteTuple = (usize, f32, u16, f32, u8, u8);

/// 将候选音符按相邻关系分组。
///
/// 分组的判定条件：同一音高（key）且当前音符起始 tick 不超过上一个音符
/// 结束 tick 加上 `proximity` 阈值。
/// 最终返回长度大于等于 2 的组（只有相邻的音符才需要合并）。
pub fn group_adjacent_notes(notes: &[NoteTuple], proximity: f32) -> Vec<Vec<NoteTuple>> {
    if notes.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<NoteTuple> = notes.to_vec();
    sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups: Vec<Vec<NoteTuple>> = Vec::new();
    for note in sorted {
        let added = match groups.last_mut() {
            Some(g) => match g.last() {
                Some(last) if last.2 == note.2 && note.1 <= last.1 + last.3 + proximity => {
                    g.push(note);
                    true
                }
                _ => false,
            },
            None => false,
        };
        if !added {
            groups.push(vec![note]);
        }
    }

    groups
        .into_iter()
        .filter(|group| group.len() >= 2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nt(index: usize, tick: f32, key: u16, length: f32) -> NoteTuple {
        (index, tick, key, length, 100, 0)
    }

    #[test]
    fn test_empty_input() {
        let groups = group_adjacent_notes(&[], 1.0);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_single_note() {
        let notes = vec![nt(0, 0.0, 60, 2.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_two_adjacent_same_key() {
        let notes = vec![nt(0, 0.0, 60, 2.0), nt(1, 2.0, 60, 2.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_two_non_adjacent_same_key() {
        let notes = vec![nt(0, 0.0, 60, 2.0), nt(1, 10.0, 60, 2.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_two_adjacent_different_key() {
        let notes = vec![nt(0, 0.0, 60, 2.0), nt(1, 2.0, 62, 2.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_unsorted_input() {
        let notes = vec![nt(1, 4.0, 60, 2.0), nt(0, 0.0, 60, 4.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0][0].0, 0);
        assert_eq!(groups[0][1].0, 1);
    }

    #[test]
    fn test_proximity_threshold() {
        // 间隔正好等于阈值时应被合并
        let notes = vec![nt(0, 0.0, 60, 2.0), nt(1, 3.0, 60, 2.0)];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn test_multiple_groups() {
        let notes = vec![
            nt(0, 0.0, 60, 2.0),
            nt(1, 2.0, 60, 2.0),
            nt(2, 10.0, 62, 2.0),
            nt(3, 12.0, 62, 2.0),
        ];
        let groups = group_adjacent_notes(&notes, 1.0);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0][0].2, 60);
        assert_eq!(groups[1][0].2, 62);
    }
}
