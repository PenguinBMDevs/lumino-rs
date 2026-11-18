//! #127：MIDI 输出端口候选顺序（从 `outputs.rs` 拆出，守住单文件 400 行）。
//!
//! 单端口打开失败（被占用/设备切换竞态）时用于回退下一个可用输出，
//! 而不是只试一个就放弃。`selected` 不在列表中时按纯枚举顺序尝试。

use lumino_midi_io::OutputInfo;

/// 按「指定设备优先、其余按枚举顺序」生成输出端口尝试顺序。
pub(super) fn ordered_output_ids(outputs: &[OutputInfo], selected: Option<u32>) -> Vec<u32> {
    let mut ids = Vec::with_capacity(outputs.len());
    if let Some(sel) = selected
        && outputs.iter().any(|o| o.id == sel)
    {
        ids.push(sel);
    }
    for output in outputs {
        if !ids.contains(&output.id) {
            ids.push(output.id);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: u32, name: &str) -> OutputInfo {
        OutputInfo {
            id,
            name: name.to_string(),
        }
    }

    /// #127：候选顺序 = 指定设备优先 + 其余按枚举顺序，且不重复、不丢失。
    #[test]
    fn test_ordered_output_ids_prefers_selected_then_enumeration() {
        let outputs = vec![output(0, "A"), output(1, "B"), output(2, "C")];
        assert_eq!(ordered_output_ids(&outputs, Some(2)), vec![2, 0, 1]);
        assert_eq!(ordered_output_ids(&outputs, Some(1)), vec![1, 0, 2]);
        assert_eq!(ordered_output_ids(&outputs, None), vec![0, 1, 2]);
        assert_eq!(
            ordered_output_ids(&outputs, Some(9)),
            vec![0, 1, 2],
            "指定设备不存在时应按枚举顺序回退"
        );
        assert!(ordered_output_ids(&[], None).is_empty());
    }
}
