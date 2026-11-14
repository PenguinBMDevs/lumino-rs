//! 拍号「单段」判定：决定 egui 骨架能否接管标尺。
//!
//! 加载器为保住 EXP-005 的逐事件往返等价，刻意全量保留同 tick 拍号
//! （多轨 MIDI 常见：每轨各写一个 tick 0 的 4/4，见
//! `midi-loader/tests/duplicate_time_signatures.rs`）。去重是显示层的责任，
//! 因此这里不能按原始条目数判断：同 tick 重复（后者覆盖前者，与 gfx 标尺
//! 的 `ts_index` 推进规则一致）与同值重复都不构成「中途变拍」。

/// 单段拍号 → `Some((分子, 分母))`；存在中途变拍 → `None`。
///
/// - 空列表 = 默认 4/4（与加载器补默认、egui 标尺回退口径一致）。
/// - 同 tick 重复拍号折叠为最后一条（gfx 标尺 `ts_index` 推进规则）。
/// - 折叠后任一拍号值与首段不同 = 中途变拍，标尺必须让位给 gfx 分段绘制。
/// - 首个拍号不在 tick 0 = tick 0 之前的默认段无法用单一小节宽表达，同样让位。
pub fn single_meter(signatures: &[(u32, u8, u8)]) -> Option<(u8, u8)> {
    let mut meter: Option<(u32, u8, u8)> = None;
    for &(tick, numerator, denominator) in signatures {
        match meter {
            // 同 tick 重复：后者覆盖前者（与 gfx 标尺一致）。
            Some((current_tick, _, _)) if current_tick == tick => {
                meter = Some((tick, numerator, denominator));
            }
            // 新的一段：必须与折叠后的首段同值，否则视为中途变拍。
            Some((current_tick, first_numerator, first_denominator)) => {
                // 契约：列表按 tick 升序（加载器排序 + 编辑入口单段写入）；
                // 乱序说明数据已不可用，交给 gfx 分段绘制兜底。
                if tick < current_tick
                    || numerator != first_numerator
                    || denominator != first_denominator
                {
                    return None;
                }
                meter = Some((tick, numerator, denominator));
            }
            // 首段必须在 tick 0：否则 tick 0 之前的默认拍号无法用单一小节宽表达。
            None => {
                if tick != 0 {
                    return None;
                }
                meter = Some((tick, numerator, denominator));
            }
        }
    }
    Some(meter.map_or((4, 4), |(_, numerator, denominator)| {
        (numerator, denominator)
    }))
}

#[cfg(test)]
mod tests {
    use super::single_meter;

    #[test]
    fn empty_is_default_common_time() {
        assert_eq!(single_meter(&[]), Some((4, 4)));
    }

    #[test]
    fn duplicate_signature_at_same_tick_collapses_to_last() {
        assert_eq!(single_meter(&[(0, 4, 4), (0, 4, 4)]), Some((4, 4)));
        assert_eq!(single_meter(&[(0, 4, 4), (0, 3, 4)]), Some((3, 4)));
    }

    #[test]
    fn repeated_same_value_is_single_meter() {
        assert_eq!(single_meter(&[(0, 4, 4), (7680, 4, 4)]), Some((4, 4)));
    }

    #[test]
    fn mid_project_change_defers_to_gfx_ruler() {
        assert_eq!(single_meter(&[(0, 4, 4), (7680, 3, 4)]), None);
        // 同 tick 折叠后再变拍：以折叠后的首段为准。
        assert_eq!(single_meter(&[(0, 4, 4), (0, 3, 4), (7680, 4, 4)]), None);
        // 乱序（违反 tick 升序契约）不可当作单段。
        assert_eq!(single_meter(&[(0, 4, 4), (7680, 4, 4), (0, 4, 4)]), None);
    }

    #[test]
    fn first_signature_after_tick_zero_defers() {
        assert_eq!(single_meter(&[(7680, 4, 4)]), None);
    }
}
