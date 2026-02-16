use std::path::Path;

use bytes::Bytes;
use encoding_rs::GB18030;
use lumino_dms::{DmsCompositeNode, DmsDataNode, DmsIntegerNode, DmsNode, DmsNodeType, write_dms_file};
use num_bigint::BigInt;

use crate::error::{ExportError, ExportResult};

/// DMS 导出选项
#[derive(Debug, Clone, Default)]
pub struct DmsExportOptions {
    /// 歌曲名称
    pub song_name: Option<String>,
    /// 版权信息
    pub copyright: Option<String>,
    /// 歌曲备注
    pub comment: Option<String>,
    /// PPQN (每四分音符脉冲数)
    pub ppqn: Option<u32>,
}

/// DMS 音符事件
#[derive(Debug, Clone)]
pub struct DmsNoteEvent {
    /// Tick 位置
    pub tick: u64,
    /// 键号 (0-127)
    pub key: u8,
    /// 力度 (0-127)
    pub velocity: u8,
    /// 门限 (tick)
    pub gate: u64,
}

/// DMS 速度事件
#[derive(Debug, Clone)]
pub struct DmsTempoEvent {
    /// Tick 位置
    pub tick: u64,
    /// 速度值 (BPM)
    pub tempo: f64,
}

/// DMS 控制事件
#[derive(Debug, Clone)]
pub struct DmsControlEvent {
    /// Tick 位置
    pub tick: u64,
    /// 控制类型 (CC 编号)
    pub control_type: u8,
    /// 控制值
    pub value: f64,
    /// 门限
    pub gate: f64,
}

/// DMS 轨道
#[derive(Debug, Clone)]
pub struct DmsTrack {
    /// 轨道名称
    pub name: Option<String>,
    /// 端口 (0-15)
    pub port: u8,
    /// 通道 (0-15)
    pub channel: u8,
    /// 是否为鼓轨道
    pub is_drum: bool,
    /// 音符事件列表
    pub notes: Vec<DmsNoteEvent>,
    /// 速度事件列表
    pub tempos: Vec<DmsTempoEvent>,
    /// 控制事件列表
    pub controls: Vec<DmsControlEvent>,
}

/// DMS 导出数据
#[derive(Debug, Clone)]
pub struct DmsExportData {
    /// 导出选项
    pub options: DmsExportOptions,
    /// 轨道列表
    pub tracks: Vec<DmsTrack>,
}

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
    let song_name = data.options.song_name.as_deref().unwrap_or("");
    let node = create_string_node(DmsNodeType::SONG_NAME, 0, song_name)?;
    root.children_mut().push(node);

    // 添加版权信息
    let copyright = data.options.copyright.as_deref().unwrap_or("");
    let node = create_string_node(DmsNodeType::SONG_COPYRIGHT, 0, copyright)?;
    root.children_mut().push(node);

    // 添加 PPQN
    if let Some(ppqn) = data.options.ppqn {
        let node = create_integer_node(DmsNodeType::SONG_PPQN, 0, ppqn as u64);
        root.children_mut().push(node);
    }

    // 添加默认的未知节点 (1007) - 4 bytes
    let node = create_data_node(DmsNodeType::UNKNOWN_1007, 0, vec![0, 0, 0, 0]);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1009) - 4 bytes
    let node = create_data_node(DmsNodeType::UNKNOWN_1009, 0, vec![0, 0, 0, 0]);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1012) - 8 bytes
    let node = create_data_node(DmsNodeType::UNKNOWN_1012, 0, vec![0; 8]);
    root.children_mut().push(node);

    // 添加工作时间 (1013)
    let node = create_integer_node(DmsNodeType::WORKING_TIME_SEC, 0, 0);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1014) - 4 bytes
    let node = create_data_node(DmsNodeType::UNKNOWN_1014, 0, vec![0, 0, 0, 0]);
    root.children_mut().push(node);

    // 添加歌曲备注 (1019)
    let comment = data.options.comment.as_deref().unwrap_or("");
    let node = create_string_node(DmsNodeType::SONG_COMMENT, 0, comment)?;
    root.children_mut().push(node);

    // 添加钢琴卷帘选中工具索引 (1020)
    let node = create_integer_node(DmsNodeType::PIANO_ROLL_SEL_NOTE_TOOL_INDEX, 0, 5);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1021) - 1 byte
    let node = create_data_node(DmsNodeType::UNKNOWN_1021, 0, vec![0]);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1022) - 1 byte
    let node = create_data_node(DmsNodeType::UNKNOWN_1022, 0, vec![0]);
    root.children_mut().push(node);

    // 添加主窗口选中工具索引 (1023)
    let node = create_integer_node(DmsNodeType::MASTER_SEL_NOTE_TOOL_INDEX, 0, 17);
    root.children_mut().push(node);

    // 添加默认的未知节点 (1024) - 1 byte
    let node = create_data_node(DmsNodeType::UNKNOWN_1024, 0, vec![0]);
    root.children_mut().push(node);

    // 添加轨道
    for track in &data.tracks {
        let track_node = build_track_node(track)?;
        root.children_mut().push(track_node);
    }

    // 添加当前变量 (1006) - 空复合节点
    let current_vars = DmsCompositeNode::new(DmsNodeType::CURRENT_VARS, 0);
    root.children_mut().push(Box::new(current_vars));

    // 添加 MIDI 输出配置 (1008)
    let midi_out_cfg = build_midi_out_cfg_node()?;
    root.children_mut().push(midi_out_cfg);

    // 添加键盘调色板 (1017)
    let key_palette = build_key_palette_node()?;
    root.children_mut().push(key_palette);

    Ok(root)
}

/// 构建 MIDI 输出配置节点
fn build_midi_out_cfg_node() -> ExportResult<Box<dyn DmsNode>> {
    let mut node = DmsCompositeNode::new(DmsNodeType::MIDI_OUT_CFG, 0);
    
    // 端口 A 配置 - 空
    let port_a = DmsDataNode::new(DmsNodeType::PORT_CFG_A, 0, Bytes::new());
    node.children_mut().push(Box::new(port_a));
    
    // 端口 B 配置 - 4 bytes
    let port_b = create_data_node(DmsNodeType::PORT_CFG_B, 0, vec![0; 4]);
    node.children_mut().push(port_b);
    
    // 端口 C 配置 - 4 bytes
    let port_c = create_data_node(DmsNodeType::PORT_CFG_C, 0, vec![0; 4]);
    node.children_mut().push(port_c);
    
    Ok(Box::new(node))
}

/// 构建键盘调色板节点
fn build_key_palette_node() -> ExportResult<Box<dyn DmsNode>> {
    let mut node = DmsCompositeNode::new(DmsNodeType::KEY_PALETTE, 0);
    
    // 添加默认的调色板配置
    for i in 0..7 {
        let child = create_data_node(DmsNodeType(i as u64 | (DmsNodeType::KEY_PALETTE.0 << 16)), 0, vec![0; if i < 2 { 1 } else if i == 3 || i == 6 { 4 } else { 1 }]);
        node.children_mut().push(child);
    }
    
    Ok(Box::new(node))
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
    let name = track.name.as_deref().unwrap_or("");
    let name_node = create_string_node(DmsNodeType::TRACK_NAME, 1, name)?;
    track_node.children_mut().push(name_node);

    // 是否静音 (1003)
    let muted_node = create_data_node(DmsNodeType::TRACK_IS_MUTED, 1, vec![0]);
    track_node.children_mut().push(muted_node);

    // 是否为鼓轨道 (1004)
    let is_drum_value = if track.is_drum { 1u64 } else { 0u64 };
    let is_drum_node = create_integer_node(DmsNodeType::TRACK_IS_DRUM, 1, is_drum_value);
    track_node.children_mut().push(is_drum_node);

    // 选中力度 (1006)
    let vel_node = create_integer_node(DmsNodeType::TRACK_SELECTED_VELOCITY, 1, 100);
    track_node.children_mut().push(vel_node);

    // 选中门限 (1007)
    let gate_node = create_integer_node(DmsNodeType::TRACK_SELECTED_GATE, 1, 0);
    track_node.children_mut().push(gate_node);

    // 鼓组名称 (1009)
    let drum_set_name = if track.is_drum { "General MIDI Drum" } else { "" };
    let drum_set_node = create_string_node(DmsNodeType::TRACK_DRUM_SET_NAME, 1, drum_set_name)?;
    track_node.children_mut().push(drum_set_node);

    // 洋葱皮数据 (1010) - 复合节点
    let onionskin_node = DmsCompositeNode::new(DmsNodeType::TRACK_ONIONSKIN_DATA, 1);
    track_node.children_mut().push(Box::new(onionskin_node));

    // Tick 补偿 (1012)
    let tick_comp_node = create_integer_node(DmsNodeType::TRACK_TICK_COMP, 1, 0);
    track_node.children_mut().push(tick_comp_node);

    // 门限补偿百分比 (1016)
    let gate_comp_node = create_data_node(DmsNodeType::TRACK_GATE_COMP_PERCENT, 1, vec![0]);
    track_node.children_mut().push(gate_comp_node);

    // 键补偿 (1017)
    let key_comp_node = create_string_node(DmsNodeType::TRACK_KEY_COMP, 1, "")?;
    track_node.children_mut().push(key_comp_node);

    // 洋葱皮颜色索引 (1018)
    let onionskin_color_node = create_data_node(DmsNodeType::TRACK_ONIONSKIN_COLOR_INDEX, 1, vec![0]);
    track_node.children_mut().push(onionskin_color_node);

    // 从小节开始的 Tick 补偿 (1019)
    let tick_comp_mea_node = create_data_node(DmsNodeType::TRACK_TICK_COMP_FROM_MEA, 1, vec![0; 2]);
    track_node.children_mut().push(tick_comp_mea_node);

    // 未知节点 (1020)
    let unknown_1020 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1020, 1, 0);
    track_node.children_mut().push(unknown_1020);

    // 未知节点 (1021)
    let unknown_1021 = create_data_node(DmsNodeType::TRACK_NOTE_RANGE_L, 1, vec![0]);
    track_node.children_mut().push(unknown_1021);

    // 未知节点 (1022)
    let unknown_1022 = create_data_node(DmsNodeType::TRACK_NOTE_RANGE_H, 1, vec![0; 2]);
    track_node.children_mut().push(unknown_1022);

    // 未知节点 (1023)
    let unknown_1023 = create_data_node(DmsNodeType::TRACK_UNKNOWN_1023, 1, vec![0]);
    track_node.children_mut().push(unknown_1023);

    // 未知节点 (1024)
    let unknown_1024 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1024, 1, 255);
    track_node.children_mut().push(unknown_1024);

    // 未知节点 (1025)
    let unknown_1025 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1025, 1, 1);
    track_node.children_mut().push(unknown_1025);

    // 未知节点 (1026)
    let unknown_1026 = create_data_node(DmsNodeType::TRACK_UNKNOWN_1026, 1, vec![0; 16]);
    track_node.children_mut().push(unknown_1026);

    // 未知节点 (1027)
    let unknown_1027 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1027, 1, 0);
    track_node.children_mut().push(unknown_1027);

    // 未知节点 (1028)
    let unknown_1028 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1028, 1, 127);
    track_node.children_mut().push(unknown_1028);

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

    // 添加轨道结束事件 (2009)
    let end_of_track_node = build_end_of_track_node()?;
    track_node.children_mut().push(end_of_track_node);

    // 未知节点 (1013)
    let unknown_1013 = create_data_node(DmsNodeType::TRACK_UNKNOWN_1013, 1, vec![0; 4]);
    track_node.children_mut().push(unknown_1013);

    // 未知节点 (1014)
    let unknown_1014 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1014, 1, 100);
    track_node.children_mut().push(unknown_1014);

    // 未知节点 (1015)
    let unknown_1015 = create_integer_node(DmsNodeType::TRACK_UNKNOWN_1015, 1, 480);
    track_node.children_mut().push(unknown_1015);

    // 未知节点 (1011) - 洋葱皮数据
    let unknown_1011 = build_onionskin_data_node()?;
    track_node.children_mut().push(unknown_1011);

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

/// 构建轨道结束事件节点
fn build_end_of_track_node() -> ExportResult<Box<dyn DmsNode>> {
    let mut event_node = DmsCompositeNode::new(DmsNodeType::END_OF_TRACK_EVENT, 1);
    
    // Tick 位置 (0 表示轨道结束)
    let tick_node = create_integer_node(DmsNodeType::ABS_TICK_POS, 2, 0);
    event_node.children_mut().push(tick_node);
    
    Ok(Box::new(event_node))
}

/// 构建洋葱皮数据节点
fn build_onionskin_data_node() -> ExportResult<Box<dyn DmsNode>> {
    let mut node = DmsCompositeNode::new(DmsNodeType::TRACK_UNKNOWN_1011, 1);
    
    // 添加两个子节点
    let child1 = create_data_node(DmsNodeType(0), 2, vec![0]);
    node.children_mut().push(child1);
    
    let child2 = create_data_node(DmsNodeType(0), 2, vec![0]);
    node.children_mut().push(child2);
    
    Ok(Box::new(node))
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

/// 创建数据节点
fn create_data_node(type_id: DmsNodeType, layer: i32, data: Vec<u8>) -> Box<dyn DmsNode> {
    Box::new(DmsDataNode::new(type_id, layer, Bytes::from(data)))
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
        lumino_dms::DmsFloatNode::new(type_id, layer, Bytes::from(buffer))
            .map_err(|e| ExportError::DmsWrite(e.to_string()))?,
    ))
}
