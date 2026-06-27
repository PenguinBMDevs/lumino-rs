//! MIDI 速度映射表

use crate::error::{ExportError, ExportResult};

/// MIDI 速度映射表：记录所有 Tempo 事件发生时的 BPM 值
pub(super) struct TempoMap {
    /// (tick, bpm)，按 tick 升序排列
    changes: Vec<(u64, f64)>,
    ppqn: u32,
}

impl TempoMap {
    /// 从预提取的 tempo 变化列表构建速度图（CompactEvent 路径，零扫描）
    pub(super) fn from_changes(changes: &[(u32, f32)], ppqn: u32) -> Self {
        let mut changes_vec: Vec<(u64, f64)> = changes
            .iter()
            .filter(|(_, bpm)| *bpm > 0.0)
            .map(|&(tick, bpm)| (tick as u64, bpm as f64))
            .collect();

        if changes_vec.is_empty() {
            changes_vec.push((0, 120.0));
        }

        changes_vec.sort_by_key(|a| a.0);
        changes_vec.dedup_by(|a, b| {
            if a.0 == b.0 {
                std::mem::swap(a, b);
                true
            } else {
                false
            }
        });
        TempoMap {
            changes: changes_vec,
            ppqn,
        }
    }

    /// 从 SMF 中扫描所有轨道的 Tempo 事件构建速度图
    pub(super) fn from_smf(smf: &midly::Smf, ppqn: u32) -> Self {
        let mut changes = vec![(0u64, 120.0f64)]; // 默认 120 BPM
        for track in &smf.tracks {
            let mut tick: u64 = 0;
            for event in track {
                tick += u32::from(event.delta) as u64;
                if let midly::TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)) = event.kind {
                    let bpm = 60_000_000.0 / tempo.as_int() as f64;
                    changes.push((tick, bpm));
                }
            }
        }
        // 按 tick 排序，相同 tick 的取最后一个（后面的轨道覆盖前面的）
        changes.sort_by_key(|a| a.0);
        changes.dedup_by(|a, b| {
            if a.0 == b.0 {
                // 保留后面的值（b 是后面的元素）
                std::mem::swap(a, b);
                true
            } else {
                false
            }
        });
        TempoMap { changes, ppqn }
    }

    /// 将 tick 转换为秒，考虑所有速度变化
    pub(super) fn tick_to_seconds(&self, tick: u64) -> f64 {
        let ppqn = self.ppqn as f64;
        let mut total = 0.0f64;
        let mut prev_tick = 0u64;
        let mut prev_bpm = 120.0;

        for &(change_tick, bpm) in &self.changes {
            if change_tick >= tick {
                // 当前速度段到目标 tick
                if tick > prev_tick {
                    let delta_ticks = (tick - prev_tick) as f64;
                    total += delta_ticks / (ppqn * prev_bpm / 60.0);
                }
                return total;
            }
            // 完整经过当前速度段
            if change_tick > prev_tick {
                let delta_ticks = (change_tick - prev_tick) as f64;
                total += delta_ticks / (ppqn * prev_bpm / 60.0);
            }
            prev_bpm = bpm;
            prev_tick = change_tick;
        }

        // 最后一段到目标 tick
        if tick > prev_tick {
            let delta_ticks = (tick - prev_tick) as f64;
            total += delta_ticks / (ppqn * prev_bpm / 60.0);
        }
        total
    }
}

/// 从 MIDI 文件头部（仅 14 字节）提取 PPQN（每四分音符脉冲数），零分配。
///
/// 当前未被直接调用（已改用 ParsedMidi.info.division），保留以备未来直接 PPQN 提取场景。
#[expect(dead_code)]
pub(super) fn extract_ppqn_from_bytes(midi_data: &[u8]) -> ExportResult<u32> {
    if midi_data.len() < 14 {
        return Err(ExportError::InvalidData(
            "MIDI 数据不足 14 字节".to_string(),
        ));
    }

    // 跳过 RIFF 包装
    let data = if &midi_data[..4] == b"RIFF" {
        midi_data
            .windows(4)
            .position(|w| w == b"MThd")
            .and_then(|pos| midi_data.get(pos..))
            .ok_or_else(|| ExportError::InvalidData("RIFF 包装中找不到 MThd".to_string()))?
    } else if &midi_data[..4] == b"MThd" {
        midi_data
    } else {
        return Err(ExportError::InvalidData("不是有效的 MIDI 文件".to_string()));
    };

    if data.len() < 14 {
        return Err(ExportError::InvalidData("MIDI 头部不完整".to_string()));
    }

    let division_raw = u16::from_be_bytes([data[12], data[13]]);
    // 最高位为 1 → SMPTE 时间码格式
    if division_raw & 0x8000 != 0 {
        Ok(480)
    } else {
        Ok((division_raw & 0x7FFF) as u32)
    }
}
