use std::path::Path;

pub fn export_midi_from_parsed_midi_sync(source_path: &Path) -> Result<Vec<u8>, String> {
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "mid" | "midi" => {
            std::fs::read(source_path).map_err(|e| format!("读取 MIDI 文件失败: {e}"))
        }
        "lmpj" => {
            // 尝试读取 LMPJ 文件内是否包含原始 MIDI 数据（有些 LMPJ 可能未保存）
            let data = std::fs::read(source_path).map_err(|e| format!("读取 LMPJ 失败: {e}"))?;
            let parsed: lumino_core::midi::ParsedMidi =
                crate::format::decode_lmpj(&data).map_err(|e| format!("解析 LMPJ 失败: {e}"))?;

            // 如果序列化数据中包含原始 midi bytes，则直接返回
            if let Some(midi_bytes) = parsed.midi_data {
                return Ok(midi_bytes);
            }

            // 否则尝试从保存的原始路径读取（如果存在）
            let original = parsed.info.path;
            if original.exists() {
                std::fs::read(&original).map_err(|e| format!("读取原始 MIDI 文件失败: {e}"))
            } else {
                Err(
                    "当前 LMPJ 未包含原始 MIDI 数据，且原始文件不存在，无法导出标准 MIDI"
                        .to_string(),
                )
            }
        }
        _ => Err(format!("不支持的 MIDI 源格式: {}", extension)),
    }
}

pub fn export_midi_from_dms_sync(source_path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(source_path).map_err(|e| format!("读取 DMS 文件失败: {e}"))?;
    let root = lumino_dms::read_dms_file(&bytes).map_err(|e| format!("解析 DMS 文件失败: {e}"))?;
    let export_data = build_midi_export_from_dms(&root);
    crate::export_midi_to_bytes(&export_data).map_err(|e| format!("导出失败: {e}"))
}

fn build_midi_export_from_dms(root: &lumino_dms::DmsCompositeNode) -> crate::midi::MidiExportData {
    use crate::midi::MidiExportOptions;
    use lumino_dms::DmsNodeType;

    let mut ppqn = 1920u16;
    let mut tracks = Vec::new();

    for root_child in root.children.iter() {
        if root_child.type_id() == DmsNodeType::SONG_PPQN
            && let Some(value) = read_u64(root_child.as_ref())
        {
            ppqn = value.clamp(1, u16::MAX as u64) as u16;
        }

        if root_child.type_id() != DmsNodeType::TRACK {
            continue;
        }

        let track_data = parse_track_from_dms(root_child.as_ref());
        tracks.push(track_data);
    }

    crate::midi::MidiExportData {
        options: MidiExportOptions { format: 1, ppqn },
        tracks,
    }
}

/// 从 DMS 节点读取 u64 值
fn read_u64(node: &dyn lumino_dms::DmsNode) -> Option<u64> {
    use lumino_dms::DmsIntegerNode;
    node.as_any()
        .downcast_ref::<DmsIntegerNode>()
        .and_then(|n| n.integer_data().to_string().parse::<u64>().ok())
}

/// 从 DMS 节点读取 f64 值
fn read_f64(node: &dyn lumino_dms::DmsNode) -> Option<f64> {
    use lumino_dms::DmsFloatNode;
    node.as_any()
        .downcast_ref::<DmsFloatNode>()
        .map(|n| n.number_data())
}

/// 从 DMS 节点读取字符串值
fn read_string(node: &dyn lumino_dms::DmsNode) -> Option<String> {
    use lumino_dms::DmsAnsiStringNode;
    node.as_any()
        .downcast_ref::<DmsAnsiStringNode>()
        .and_then(|n| n.string_data().ok())
}

/// 按类型查找子节点
fn child_by_type(
    node: &dyn lumino_dms::DmsNode,
    ty: lumino_dms::DmsNodeType,
) -> Option<&dyn lumino_dms::DmsNode> {
    node.children()
        .iter()
        .find(|child| child.type_id() == ty)
        .map(|child| child.as_ref())
}

/// 从子节点读取 u64 值，带默认值和范围限制
fn read_child_u64_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    child_by_type(parent, node_type)
        .and_then(read_u64)
        .unwrap_or(default)
        .clamp(min, max)
}

/// 从子节点读取 u32 值，带默认值和范围限制
fn read_child_u32_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: u64,
    min: u64,
    max: u64,
) -> u32 {
    read_child_u64_clamped(parent, node_type, default, min, max) as u32
}

/// 从子节点读取 f64 值，带默认值和范围限制
fn read_child_f64_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: f64,
    min: f64,
    max: Option<f64>,
) -> f64 {
    child_by_type(parent, node_type)
        .and_then(read_f64)
        .unwrap_or(default)
        .clamp(min, max.unwrap_or(f64::MAX))
}

/// 从 DMS 节点解析单个音轨数据
fn parse_track_from_dms(track_node: &dyn lumino_dms::DmsNode) -> crate::midi::MidiTrackData {
    use crate::midi::MidiTrackData;
    use lumino_dms::DmsNodeType;

    let mut channel = 0u8;
    let mut name = None;
    let mut notes = Vec::new();
    let mut tempos = Vec::new();
    let mut control_changes = Vec::new();

    for track_child in track_node.children().iter() {
        match track_child.type_id() {
            DmsNodeType::TRACK_CHANNEL => {
                if let Some(ch) = read_u64(track_child.as_ref()) {
                    channel = ch.min(15) as u8;
                }
            }
            DmsNodeType::TRACK_NAME => {
                name = read_string(track_child.as_ref());
            }
            DmsNodeType::NOTE_EVENT => {
                if let Some(event) = parse_note_event(track_child.as_ref(), channel) {
                    notes.push(event);
                }
            }
            DmsNodeType::TEMPO_EVENT => {
                if let Some(event) = parse_tempo_event(track_child.as_ref()) {
                    tempos.push(event);
                }
            }
            DmsNodeType::CONTROL_EVENT => {
                if let Some(event) = parse_control_event(track_child.as_ref(), channel) {
                    control_changes.push(event);
                }
            }
            _ => {}
        }
    }

    MidiTrackData {
        notes,
        tempos,
        program_changes: Vec::new(),
        control_changes,
        time_signatures: Vec::new(),
        key_signatures: Vec::new(),
        name,
    }
}

/// 解析音符事件
fn parse_note_event(
    event_node: &dyn lumino_dms::DmsNode,
    channel: u8,
) -> Option<crate::midi::MidiNoteEvent> {
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let key = read_child_u32_clamped(event_node, DmsNodeType::NOTE_KEY_NUMBER, 60, 0, 127) as u8;
    let velocity =
        read_child_u32_clamped(event_node, DmsNodeType::NOTE_VELOCITY, 100, 0, 127) as u8;
    let duration =
        read_child_u32_clamped(event_node, DmsNodeType::NOTE_GATE, 1, 1, u32::MAX as u64);

    Some(crate::midi::MidiNoteEvent {
        tick,
        channel,
        key,
        velocity,
        duration,
    })
}

/// 解析速度事件
fn parse_tempo_event(event_node: &dyn lumino_dms::DmsNode) -> Option<crate::midi::MidiTempoEvent> {
    use crate::midi::{MidiTempoEvent, bpm_to_tempo};
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let bpm = read_child_f64_clamped(event_node, DmsNodeType::TEMPO_VALUE, 120.0, 1.0, None);

    Some(MidiTempoEvent {
        tick,
        tempo: bpm_to_tempo(bpm),
    })
}

/// 解析控制变更事件
fn parse_control_event(
    event_node: &dyn lumino_dms::DmsNode,
    channel: u8,
) -> Option<crate::midi::MidiControlChangeEvent> {
    use crate::midi::MidiControlChangeEvent;
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let controller = read_child_u32_clamped(event_node, DmsNodeType::CONTROL_TYPE, 0, 0, 127) as u8;
    let value = read_child_f64_clamped(
        event_node,
        DmsNodeType::CONTROL_VALUE,
        0.0,
        0.0,
        Some(127.0),
    )
    .round() as u8;

    Some(MidiControlChangeEvent {
        tick,
        channel,
        controller,
        value,
    })
}
