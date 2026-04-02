//! DMS 导出功能
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（DmsExportOptions, DmsNoteEvent, DmsTrack 等）

use std::path::Path;

use bytes::Bytes;
use encoding_rs::GB18030;
use lumino_dms::{
    DmsCompositeNode, DmsFloatNode, DmsIntegerNode, DmsNode, DmsNodeType, write_dms_file,
};
use num_bigint::BigInt;

pub mod types;

pub use types::{
    DmsControlEvent, DmsExportData, DmsExportOptions, DmsNoteEvent, DmsTempoEvent, DmsTrack,
};

use crate::error::{ExportError, ExportResult};

/// 导出为 DMS 文件
pub fn export_dms<P: AsRef<Path>>(path: P, data: &DmsExportData) -> ExportResult<()> {
    let root = build_dms_tree(data)?;
    let bytes = write_dms_file(&root).map_err(|e| ExportError::DmsWrite(e.to_string()))?;
    std::fs::write(path.as_ref(), bytes)?;
    Ok(())
}

/// 导出 DMS 到字节数组
pub fn export_dms_to_bytes(data: &DmsExportData) -> ExportResult<Vec<u8>> {
    let root = build_dms_tree(data)?;
    write_dms_file(&root).map_err(|e| ExportError::DmsWrite(e.to_string()))
}

/// 构建 DMS 节点树
fn build_dms_tree(data: &DmsExportData) -> ExportResult<DmsCompositeNode> {
    let mut root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);

    // 添加歌曲名称
    if let Some(ref name) = data.options.song_name {
        let node = create_string_node(DmsNodeType::SONG_NAME, 0, name)?;
        root.children_mut().push(node);
    }

    // 添加版权信息
    if let Some(ref copyright) = data.options.copyright {
        let node = create_string_node(DmsNodeType::SONG_COPYRIGHT, 0, copyright)?;
        root.children_mut().push(node);
    }

    // 添加歌曲备注
    if let Some(ref comment) = data.options.comment {
        let node = create_string_node(DmsNodeType::SONG_COMMENT, 0, comment)?;
        root.children_mut().push(node);
    }

    // 添加 PPQN
    if let Some(ppqn) = data.options.ppqn {
        let node = create_integer_node(DmsNodeType::SONG_PPQN, 0, ppqn as u64);
        root.children_mut().push(node);
    }

    // 添加轨道
    for track in &data.tracks {
        let track_node = build_track_node(track)?;
        root.children_mut().push(track_node);
    }

    Ok(root)
}

/// 构建轨道节点
fn build_track_node(track: &DmsTrack) -> ExportResult<Box<dyn DmsNode>> {
    let mut track_node = DmsCompositeNode::new(DmsNodeType::TRACK, 0);

    // 端口
    let port_node = create_integer_node(DmsNodeType::TRACK_PORT, 1, track.port as u64);
    track_node.children_mut().push(port_node);

    // 通道
    let channel_node = create_integer_node(DmsNodeType::TRACK_CHANNEL, 1, track.channel as u64);
    track_node.children_mut().push(channel_node);

    // 轨道名称
    if let Some(ref name) = track.name {
        let name_node = create_string_node(DmsNodeType::TRACK_NAME, 1, name)?;
        track_node.children_mut().push(name_node);
    }

    // 添加音符事件
    for note in &track.notes {
        let note_node = build_note_event_node(note)?;
        track_node.children_mut().push(note_node);
    }

    // 添加速度事件
    for tempo in &track.tempos {
        let tempo_node = build_tempo_event_node(tempo)?;
        track_node.children_mut().push(tempo_node);
    }

    // 添加控制事件
    for control in &track.controls {
        let control_node = build_control_event_node(control)?;
        track_node.children_mut().push(control_node);
    }

    Ok(Box::new(track_node))
}

/// 构建音符事件节点
fn build_note_event_node(note: &DmsNoteEvent) -> ExportResult<Box<dyn DmsNode>> {
    let mut event_node = DmsCompositeNode::new(DmsNodeType::NOTE_EVENT, 1);

    // Tick 位置
    let tick_node = create_integer_node(DmsNodeType::ABS_TICK_POS, 2, note.tick);
    event_node.children_mut().push(tick_node);

    // 键号
    let key_node = create_integer_node(DmsNodeType::NOTE_KEY_NUMBER, 2, note.key as u64);
    event_node.children_mut().push(key_node);

    // 力度
    let velocity_node = create_integer_node(DmsNodeType::NOTE_VELOCITY, 2, note.velocity as u64);
    event_node.children_mut().push(velocity_node);

    // 门限
    let gate_node = create_integer_node(DmsNodeType::NOTE_GATE, 2, note.gate);
    event_node.children_mut().push(gate_node);

    Ok(Box::new(event_node))
}

/// 构建速度事件节点
fn build_tempo_event_node(tempo: &DmsTempoEvent) -> ExportResult<Box<dyn DmsNode>> {
    let mut event_node = DmsCompositeNode::new(DmsNodeType::TEMPO_EVENT, 1);

    // Tick 位置
    let tick_node = create_integer_node(DmsNodeType::ABS_TICK_POS, 2, tempo.tick);
    event_node.children_mut().push(tick_node);

    // 速度值
    let tempo_node = create_float_node(DmsNodeType::TEMPO_VALUE, 2, tempo.tempo)?;
    event_node.children_mut().push(tempo_node);

    Ok(Box::new(event_node))
}

/// 构建控制事件节点
fn build_control_event_node(control: &DmsControlEvent) -> ExportResult<Box<dyn DmsNode>> {
    let mut event_node = DmsCompositeNode::new(DmsNodeType::CONTROL_EVENT, 1);

    // Tick 位置
    let tick_node = create_integer_node(DmsNodeType::ABS_TICK_POS, 2, control.tick);
    event_node.children_mut().push(tick_node);

    // 控制类型
    let type_node = create_integer_node(DmsNodeType::CONTROL_TYPE, 2, control.control_type as u64);
    event_node.children_mut().push(type_node);

    // 控制值
    let value_node = create_float_node(DmsNodeType::CONTROL_VALUE, 2, control.value)?;
    event_node.children_mut().push(value_node);

    // 门限
    let gate_node = create_float_node(DmsNodeType::CONTROL_GATE, 2, control.gate)?;
    event_node.children_mut().push(gate_node);

    Ok(Box::new(event_node))
}

/// 创建字符串节点
fn create_string_node(
    type_id: DmsNodeType,
    layer: i32,
    value: &str,
) -> ExportResult<Box<dyn DmsNode>> {
    let (encoded, _, had_errors) = GB18030.encode(value);
    if had_errors {
        return Err(ExportError::Encoding(format!(
            "无法编码字符串为 GB18030: {}",
            value
        )));
    }
    Ok(Box::new(lumino_dms::DmsAnsiStringNode::new(
        type_id,
        layer,
        Bytes::from(encoded.to_vec()),
    )))
}

/// 创建整数节点
fn create_integer_node(type_id: DmsNodeType, layer: i32, value: u64) -> Box<dyn DmsNode> {
    let mut int_node = DmsIntegerNode::new(type_id, layer, Bytes::new());
    int_node.set_integer_data(&BigInt::from(value));
    Box::new(int_node)
}

/// 创建浮点数节点
fn create_float_node(
    type_id: DmsNodeType,
    layer: i32,
    value: f64,
) -> ExportResult<Box<dyn DmsNode>> {
    // DMS 浮点节点格式：6字节内部头 + 数据
    // 内部头：type_field (2字节, 值为0) + length_field (4字节, 值为8)
    // 数据：双精度浮点值 (8字节)
    // 总长度：14 字节
    const HEADER_SIZE: usize = 6;
    let mut buffer = vec![0u8; HEADER_SIZE + 8];

    // 设置内部头的长度字段 (offset 2-5)
    buffer[2..6].copy_from_slice(&8u32.to_le_bytes());
    // 设置浮点数值 (offset 6-13)
    buffer[HEADER_SIZE..HEADER_SIZE + 8].copy_from_slice(&value.to_le_bytes());

    Ok(Box::new(
        DmsFloatNode::new(type_id, layer, Bytes::from(buffer))
            .map_err(|e| ExportError::DmsWrite(e.to_string()))?,
    ))
}

// TODO: create_data_node 暂时保留，未来可能用于扩展数据节点类型支持
// 目前未被调用，已标记为允许死代码
#[allow(dead_code)]
fn _create_data_node(type_id: DmsNodeType, layer: i32, data: Vec<u8>) -> Box<dyn DmsNode> {
    Box::new(lumino_dms::DmsDataNode::new(
        type_id,
        layer,
        Bytes::from(data),
    ))
}
