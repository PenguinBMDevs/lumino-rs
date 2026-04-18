use super::DmsNodeType;

impl DmsNodeType {
    /// 根节点
    pub const ROOT: Self = Self(0x0000);

    // 根级别节点
    /// 歌曲名称 (原始ID: 1000)
    pub const SONG_NAME: Self = Self(1000);
    /// 版权信息 (原始ID: 1001)
    pub const SONG_COPYRIGHT: Self = Self(1001);
    /// PPQN（每四分音符脉冲数）(原始ID: 1002)
    pub const SONG_PPQN: Self = Self(1002);
    /// 轨道 (原始ID: 1003)
    pub const TRACK: Self = Self(1003);
    /// 当前变量 (原始ID: 1006)
    pub const CURRENT_VARS: Self = Self(1006);
    /// MIDI 输出配置 (原始ID: 1008)
    pub const MIDI_OUT_CFG: Self = Self(1008);
    /// 工作时间（秒）(原始ID: 1013)
    pub const WORKING_TIME_SEC: Self = Self(1013);
    /// 键盘调色板 (原始ID: 1017)
    pub const KEY_PALETTE: Self = Self(1017);
    /// 端口配置 (原始ID: 1018)
    pub const PORT_CFG: Self = Self(1018);
    /// 歌曲注释 (原始ID: 1019)
    pub const SONG_COMMENT: Self = Self(1019);
    /// 钢琴卷帘选中工具索引 (原始ID: 1020)
    pub const PIANO_ROLL_SEL_NOTE_TOOL_INDEX: Self = Self(1020);
    /// 主窗口选中工具索引 (原始ID: 1023)
    pub const MASTER_SEL_NOTE_TOOL_INDEX: Self = Self(1023);

    // 根级别未知/其他节点
    /// 未知节点 1007
    pub const UNKNOWN_1007: Self = Self(1007);
    /// 未知节点 1009
    pub const UNKNOWN_1009: Self = Self(1009);
    /// 未知节点 1012
    pub const UNKNOWN_1012: Self = Self(1012);
    /// 未知节点 1014
    pub const UNKNOWN_1014: Self = Self(1014);
    /// 未知节点 1021
    pub const UNKNOWN_1021: Self = Self(1021);
    /// 未知节点 1022
    pub const UNKNOWN_1022: Self = Self(1022);
    /// 未知节点 1024
    pub const UNKNOWN_1024: Self = Self(1024);

    // 端口配置（PORT_CFG 子节点）
    pub const PORT_CFG_A: Self = Self(1000 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_B: Self = Self(1001 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_C: Self = Self(1002 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_D: Self = Self(1003 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_E: Self = Self(1004 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_F: Self = Self(1005 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_G: Self = Self(1006 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_H: Self = Self(1007 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_I: Self = Self(1008 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_J: Self = Self(1009 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_K: Self = Self(1010 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_L: Self = Self(1011 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_M: Self = Self(1012 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_N: Self = Self(1013 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_O: Self = Self(1014 | (Self::PORT_CFG.0 << 16));
    pub const PORT_CFG_P: Self = Self(1015 | (Self::PORT_CFG.0 << 16));

    // 轨道属性（TRACK 子节点）
    /// 轨道端口
    pub const TRACK_PORT: Self = Self(1000 | (Self::TRACK.0 << 16));
    /// 轨道通道
    pub const TRACK_CHANNEL: Self = Self(1001 | (Self::TRACK.0 << 16));
    /// 轨道名称
    pub const TRACK_NAME: Self = Self(1002 | (Self::TRACK.0 << 16));
    /// 轨道静音状态
    pub const TRACK_IS_MUTED: Self = Self(1003 | (Self::TRACK.0 << 16));
    /// 是否为鼓轨道
    pub const TRACK_IS_DRUM: Self = Self(1004 | (Self::TRACK.0 << 16));
    /// 选中力度
    pub const TRACK_SELECTED_VELOCITY: Self = Self(1006 | (Self::TRACK.0 << 16));
    /// 选中门限
    pub const TRACK_SELECTED_GATE: Self = Self(1007 | (Self::TRACK.0 << 16));
    /// 鼓组名称
    pub const TRACK_DRUM_SET_NAME: Self = Self(1009 | (Self::TRACK.0 << 16));
    /// 洋葱皮数据
    pub const TRACK_ONIONSKIN_DATA: Self = Self(1010 | (Self::TRACK.0 << 16));
    /// Tick 补偿
    pub const TRACK_TICK_COMP: Self = Self(1012 | (Self::TRACK.0 << 16));
    /// 门限补偿百分比
    pub const TRACK_GATE_COMP_PERCENT: Self = Self(1016 | (Self::TRACK.0 << 16));
    /// 键补偿
    pub const TRACK_KEY_COMP: Self = Self(1017 | (Self::TRACK.0 << 16));
    /// 洋葱皮颜色索引
    pub const TRACK_ONIONSKIN_COLOR_INDEX: Self = Self(1018 | (Self::TRACK.0 << 16));
    /// 从小节开始的 Tick 补偿
    pub const TRACK_TICK_COMP_FROM_MEA: Self = Self(1019 | (Self::TRACK.0 << 16));
    /// 音符范围下限
    pub const TRACK_NOTE_RANGE_L: Self = Self(1021 | (Self::TRACK.0 << 16));
    /// 音符范围上限
    pub const TRACK_NOTE_RANGE_H: Self = Self(1022 | (Self::TRACK.0 << 16));

    // 轨道未知节点
    pub const TRACK_UNKNOWN_1005: Self = Self(1005 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1008: Self = Self(1008 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1011: Self = Self(1011 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1013: Self = Self(1013 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1014: Self = Self(1014 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1015: Self = Self(1015 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1020: Self = Self(1020 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1023: Self = Self(1023 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1024: Self = Self(1024 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1025: Self = Self(1025 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1026: Self = Self(1026 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1027: Self = Self(1027 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1028: Self = Self(1028 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1029: Self = Self(1029 | (Self::TRACK.0 << 16));
    pub const TRACK_UNKNOWN_1030: Self = Self(1030 | (Self::TRACK.0 << 16));

    // 事件类型（TRACK 子节点）
    /// 音符事件
    pub const NOTE_EVENT: Self = Self(2001 | (Self::TRACK.0 << 16));
    /// 程序变更事件
    pub const PROGRAM_CHANGE_EVENT: Self = Self(2002 | (Self::TRACK.0 << 16));
    /// 控制事件
    pub const CONTROL_EVENT: Self = Self(2003 | (Self::TRACK.0 << 16));
    /// 自定义 `SysEx` 事件
    pub const CUSTOM_SYSEX_EVENT: Self = Self(2004 | (Self::TRACK.0 << 16));
    /// 注释事件
    pub const COMMENT_EVENT: Self = Self(2005 | (Self::TRACK.0 << 16));
    /// 公式事件
    pub const FORMULA_EVENT: Self = Self(2007 | (Self::TRACK.0 << 16));
    /// 速度事件
    pub const TEMPO_EVENT: Self = Self(2008 | (Self::TRACK.0 << 16));
    /// 轨道结束事件
    pub const END_OF_TRACK_EVENT: Self = Self(2009 | (Self::TRACK.0 << 16));
    /// 歌词事件
    pub const LYRICS_EVENT: Self = Self(2011 | (Self::TRACK.0 << 16));
    /// 提示点事件
    pub const CUE_POINT_EVENT: Self = Self(2012 | (Self::TRACK.0 << 16));
    /// 小节链接事件
    pub const MEASURE_LINK_EVENT: Self = Self(2014 | (Self::TRACK.0 << 16));
    /// 拍号事件
    pub const TIME_SIG_EVENT: Self = Self(2015 | (Self::TRACK.0 << 16));
    /// 调号事件
    pub const KEY_SIG_EVENT: Self = Self(2016 | (Self::TRACK.0 << 16));
    /// 标记事件
    pub const MARKER_EVENT: Self = Self(2017 | (Self::TRACK.0 << 16));
    /// 音阶事件
    pub const SCALE_EVENT: Self = Self(2018 | (Self::TRACK.0 << 16));
    /// 和弦事件
    pub const CHORD_EVENT: Self = Self(2019 | (Self::TRACK.0 << 16));

    /// 绝对 Tick 位置
    pub const ABS_TICK_POS: Self = Self(1001 | (Self::TRACK.0 << 32));

    // 音符事件属性
    pub const NOTE_KEY_NUMBER: Self = Self(2001 | (Self::NOTE_EVENT.0 << 16));
    pub const NOTE_VELOCITY: Self = Self(2002 | (Self::NOTE_EVENT.0 << 16));
    pub const NOTE_GATE: Self = Self(2003 | (Self::NOTE_EVENT.0 << 16));

    // 控制事件属性
    pub const CONTROL_TYPE: Self = Self(2001 | (Self::CONTROL_EVENT.0 << 16));
    pub const CONTROL_GATE: Self = Self(2002 | (Self::CONTROL_EVENT.0 << 16));
    pub const CONTROL_VALUE: Self = Self(2003 | (Self::CONTROL_EVENT.0 << 16));

    // 注释事件属性
    pub const COMMENT_TEXT: Self = Self(2001 | (Self::COMMENT_EVENT.0 << 16));

    // 公式事件属性
    pub const FORMULA_VAR_NAME: Self = Self(2001 | (Self::FORMULA_EVENT.0 << 16));
    pub const FORMULA_EXPRESSION: Self = Self(2002 | (Self::FORMULA_EVENT.0 << 16));

    // 速度事件属性
    pub const TEMPO_VALUE: Self = Self(2001 | (Self::TEMPO_EVENT.0 << 16));

    // SysEx 事件属性
    pub const CUSTOM_SYSEX_DATA: Self = Self(2001 | (Self::CUSTOM_SYSEX_EVENT.0 << 16));

    // 歌词事件属性
    pub const LYRICS_LYRICS: Self = Self(2001 | (Self::LYRICS_EVENT.0 << 16));

    // 提示点事件属性
    pub const CUE_POINT_VALUE: Self = Self(2001 | (Self::CUE_POINT_EVENT.0 << 16));

    // 小节链接事件属性
    pub const MEASURE_LINK_MEASURE: Self = Self(2001 | (Self::MEASURE_LINK_EVENT.0 << 16));
    pub const MEASURE_LINK_KEY_COMP: Self = Self(2002 | (Self::MEASURE_LINK_EVENT.0 << 16));

    // 调号事件属性
    pub const KEY_SIG_INDEX: Self = Self(2001 | (Self::KEY_SIG_EVENT.0 << 16));

    // 拍号事件属性
    pub const TIME_SIG_NUMERATOR: Self = Self(2001 | (Self::TIME_SIG_EVENT.0 << 16));
    pub const TIME_SIG_DENOMINATOR: Self = Self(2002 | (Self::TIME_SIG_EVENT.0 << 16));

    // 标记事件属性
    pub const MARKER_NAME: Self = Self(2001 | (Self::MARKER_EVENT.0 << 16));

    // 程序变更事件属性
    pub const PROGRAM_CHANGE_PROGRAM: Self = Self(2001 | (Self::PROGRAM_CHANGE_EVENT.0 << 16));
}
