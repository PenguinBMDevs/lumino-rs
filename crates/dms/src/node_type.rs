//! DMS 节点类型定义

/// DMS 节点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DmsNodeType(pub u64);

impl DmsNodeType {
    /// 根节点
    pub const ROOT: Self = Self(0x0000);

    // === 根级别节点 ===
    /// 歌曲名称
    pub const SONG_NAME: Self = Self(1000);
    /// 版权信息
    pub const SONG_COPYRIGHT: Self = Self(1001);
    /// PPQN（每四分音符脉冲数）
    pub const SONG_PPQN: Self = Self(1002);
    /// 轨道
    pub const TRACK: Self = Self(1003);
    /// 当前变量
    pub const CURRENT_VARS: Self = Self(1006);
    /// MIDI 输出配置
    pub const MIDI_OUT_CFG: Self = Self(1008);
    /// 工作时间（秒）
    pub const WORKING_TIME_SEC: Self = Self(1013);
    /// 键盘调色板
    pub const KEY_PALETTE: Self = Self(1017);
    /// 端口配置
    pub const PORT_CFG: Self = Self(1018);
    /// 歌曲注释
    pub const SONG_COMMENT: Self = Self(1019);
    /// 钢琴卷帘选中工具索引
    pub const PIANO_ROLL_SEL_NOTE_TOOL_INDEX: Self = Self(1020);
    /// 主窗口选中工具索引
    pub const MASTER_SEL_NOTE_TOOL_INDEX: Self = Self(1023);

    // === 端口配置（PORT_CFG 子节点）===
    /// 端口 A 配置
    pub const PORT_CFG_A: Self = Self(1000 | (Self::PORT_CFG.0 << 16));
    /// 端口 B 配置
    pub const PORT_CFG_B: Self = Self(1001 | (Self::PORT_CFG.0 << 16));
    /// 端口 C 配置
    pub const PORT_CFG_C: Self = Self(1002 | (Self::PORT_CFG.0 << 16));
    /// 端口 D 配置
    pub const PORT_CFG_D: Self = Self(1003 | (Self::PORT_CFG.0 << 16));
    /// 端口 E 配置
    pub const PORT_CFG_E: Self = Self(1004 | (Self::PORT_CFG.0 << 16));
    /// 端口 F 配置
    pub const PORT_CFG_F: Self = Self(1005 | (Self::PORT_CFG.0 << 16));
    /// 端口 G 配置
    pub const PORT_CFG_G: Self = Self(1006 | (Self::PORT_CFG.0 << 16));
    /// 端口 H 配置
    pub const PORT_CFG_H: Self = Self(1007 | (Self::PORT_CFG.0 << 16));
    /// 端口 I 配置
    pub const PORT_CFG_I: Self = Self(1008 | (Self::PORT_CFG.0 << 16));
    /// 端口 J 配置
    pub const PORT_CFG_J: Self = Self(1009 | (Self::PORT_CFG.0 << 16));
    /// 端口 K 配置
    pub const PORT_CFG_K: Self = Self(1010 | (Self::PORT_CFG.0 << 16));
    /// 端口 L 配置
    pub const PORT_CFG_L: Self = Self(1011 | (Self::PORT_CFG.0 << 16));
    /// 端口 M 配置
    pub const PORT_CFG_M: Self = Self(1012 | (Self::PORT_CFG.0 << 16));
    /// 端口 N 配置
    pub const PORT_CFG_N: Self = Self(1013 | (Self::PORT_CFG.0 << 16));
    /// 端口 O 配置
    pub const PORT_CFG_O: Self = Self(1014 | (Self::PORT_CFG.0 << 16));
    /// 端口 P 配置
    pub const PORT_CFG_P: Self = Self(1015 | (Self::PORT_CFG.0 << 16));

    // === 轨道属性（TRACK 子节点）===
    /// 轨道端口
    pub const TRACK_PORT: Self = Self(1000 | (Self::TRACK.0 << 16));
    /// 轨道通道
    pub const TRACK_CHANNEL: Self = Self(1001 | (Self::TRACK.0 << 16));
    /// 轨道名称
    pub const TRACK_NAME: Self = Self(1002 | (Self::TRACK.0 << 16));
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
    /// 从小节开始的 Tick 补偿
    pub const TRACK_TICK_COMP_FROM_MEA: Self = Self(1019 | (Self::TRACK.0 << 16));
    /// 门限补偿百分比
    pub const TRACK_GATE_COMP_PERCENT: Self = Self(1016 | (Self::TRACK.0 << 16));
    /// 键补偿
    pub const TRACK_KEY_COMP: Self = Self(1017 | (Self::TRACK.0 << 16));
    /// 洋葱皮颜色索引
    pub const TRACK_ONIONSKIN_COLOR_INDEX: Self = Self(1018 | (Self::TRACK.0 << 16));
    /// 音符范围下限
    pub const TRACK_NOTE_RANGE_L: Self = Self(1021 | (Self::TRACK.0 << 16));
    /// 音符范围上限
    pub const TRACK_NOTE_RANGE_H: Self = Self(1022 | (Self::TRACK.0 << 16));

    // === 事件类型（TRACK 子节点）===
    /// 音符事件
    pub const NOTE_EVENT: Self = Self(2001 | (Self::TRACK.0 << 16));
    /// 程序变更事件
    pub const PROGRAM_CHANGE_EVENT: Self = Self(2002 | (Self::TRACK.0 << 16));
    /// 控制事件
    pub const CONTROL_EVENT: Self = Self(2003 | (Self::TRACK.0 << 16));
    /// 自定义 SysEx 事件
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

    // === 音符事件属性 ===
    /// 音符键号
    pub const NOTE_KEY_NUMBER: Self = Self(2001 | (Self::NOTE_EVENT.0 << 16));
    /// 音符力度
    pub const NOTE_VELOCITY: Self = Self(2002 | (Self::NOTE_EVENT.0 << 16));
    /// 音符门限
    pub const NOTE_GATE: Self = Self(2003 | (Self::NOTE_EVENT.0 << 16));

    // === 控制事件属性 ===
    /// 控制类型
    pub const CONTROL_TYPE: Self = Self(2001 | (Self::CONTROL_EVENT.0 << 16));
    /// 控制门限
    pub const CONTROL_GATE: Self = Self(2002 | (Self::CONTROL_EVENT.0 << 16));
    /// 控制值
    pub const CONTROL_VALUE: Self = Self(2003 | (Self::CONTROL_EVENT.0 << 16));

    // === 注释事件属性 ===
    /// 注释文本
    pub const COMMENT_TEXT: Self = Self(2001 | (Self::COMMENT_EVENT.0 << 16));

    // === 公式事件属性 ===
    /// 变量名
    pub const FORMULA_VAR_NAME: Self = Self(2001 | (Self::FORMULA_EVENT.0 << 16));
    /// 表达式
    pub const FORMULA_EXPRESSION: Self = Self(2002 | (Self::FORMULA_EVENT.0 << 16));

    // === 速度事件属性 ===
    /// 速度值
    pub const TEMPO_VALUE: Self = Self(2001 | (Self::TEMPO_EVENT.0 << 16));

    // === SysEx 事件属性 ===
    /// SysEx 数据
    pub const CUSTOM_SYSEX_DATA: Self = Self(2001 | (Self::CUSTOM_SYSEX_EVENT.0 << 16));

    // === 歌词事件属性 ===
    /// 歌词文本
    pub const LYRICS_LYRICS: Self = Self(2001 | (Self::LYRICS_EVENT.0 << 16));

    // === 提示点事件属性 ===
    /// 提示点值
    pub const CUE_POINT_VALUE: Self = Self(2001 | (Self::CUE_POINT_EVENT.0 << 16));

    // === 小节链接事件属性 ===
    /// 链接小节
    pub const MEASURE_LINK_MEASURE: Self = Self(2001 | (Self::MEASURE_LINK_EVENT.0 << 16));
    /// 链接键补偿
    pub const MEASURE_LINK_KEY_COMP: Self = Self(2002 | (Self::MEASURE_LINK_EVENT.0 << 16));

    // === 调号事件属性 ===
    /// 调号索引
    pub const KEY_SIG_INDEX: Self = Self(2001 | (Self::KEY_SIG_EVENT.0 << 16));

    // === 拍号事件属性 ===
    /// 分子
    pub const TIME_SIG_NUMERATOR: Self = Self(2001 | (Self::TIME_SIG_EVENT.0 << 16));
    /// 分母
    pub const TIME_SIG_DENOMINATOR: Self = Self(2002 | (Self::TIME_SIG_EVENT.0 << 16));

    // === 标记事件属性 ===
    /// 标记名称
    pub const MARKER_NAME: Self = Self(2001 | (Self::MARKER_EVENT.0 << 16));

    /// 从原始类型 ID 和父节点构建完整类型
    pub fn from_parts(type_id: u16, _layer: i32, parent: Option<&DmsNodeType>) -> Self {
        if let Some(parent) = parent {
            let result = type_id as u64 | (parent.0 << 16);
            // AbsTickPos 特殊处理
            let parent_high = (result & 0x0000_0000_FFFF_0000) >> 16;
            let parent_low = result & 0xFFFF_FFFF_0000_FFFF;
            if parent_high >= 2000 && parent_low == Self::ABS_TICK_POS.0 & 0xFFFF_FFFF_0000_FFFF {
                return Self::ABS_TICK_POS;
            }
            Self(result)
        } else {
            Self(type_id as u64)
        }
    }

    /// 是否为复合节点
    pub fn is_composite(&self) -> bool {
        matches!(
            *self,
            Self::ROOT
                | Self::CURRENT_VARS
                | Self::MIDI_OUT_CFG
                | Self::TRACK
                | Self::KEY_PALETTE
                | Self::PORT_CFG
                | Self::PORT_CFG_A
                | Self::PORT_CFG_B
                | Self::PORT_CFG_C
                | Self::PORT_CFG_D
                | Self::PORT_CFG_E
                | Self::PORT_CFG_F
                | Self::PORT_CFG_G
                | Self::PORT_CFG_H
                | Self::PORT_CFG_I
                | Self::PORT_CFG_J
                | Self::PORT_CFG_K
                | Self::PORT_CFG_L
                | Self::PORT_CFG_M
                | Self::PORT_CFG_N
                | Self::PORT_CFG_O
                | Self::PORT_CFG_P
                | Self::TRACK_ONIONSKIN_DATA
                | Self::NOTE_EVENT
                | Self::PROGRAM_CHANGE_EVENT
                | Self::CONTROL_EVENT
                | Self::CUSTOM_SYSEX_EVENT
                | Self::COMMENT_EVENT
                | Self::FORMULA_EVENT
                | Self::TEMPO_EVENT
                | Self::END_OF_TRACK_EVENT
                | Self::LYRICS_EVENT
                | Self::CUE_POINT_EVENT
                | Self::MEASURE_LINK_EVENT
                | Self::TIME_SIG_EVENT
                | Self::KEY_SIG_EVENT
                | Self::MARKER_EVENT
                | Self::SCALE_EVENT
                | Self::CHORD_EVENT
        )
    }

    /// 是否为字符串节点
    pub fn is_string(&self) -> bool {
        matches!(
            *self,
            Self::SONG_NAME
                | Self::SONG_COPYRIGHT
                | Self::SONG_COMMENT
                | Self::TRACK_NAME
                | Self::TRACK_DRUM_SET_NAME
                | Self::COMMENT_TEXT
                | Self::FORMULA_VAR_NAME
                | Self::FORMULA_EXPRESSION
                | Self::CUSTOM_SYSEX_DATA
                | Self::LYRICS_LYRICS
                | Self::CUE_POINT_VALUE
                | Self::MARKER_NAME
        )
    }

    /// 是否为整数节点
    pub fn is_integer(&self) -> bool {
        matches!(
            *self,
            Self::SONG_PPQN
                | Self::PIANO_ROLL_SEL_NOTE_TOOL_INDEX
                | Self::MASTER_SEL_NOTE_TOOL_INDEX
                | Self::WORKING_TIME_SEC
                | Self::TRACK_PORT
                | Self::TRACK_CHANNEL
                | Self::TRACK_IS_DRUM
                | Self::TRACK_SELECTED_VELOCITY
                | Self::TRACK_SELECTED_GATE
                | Self::TRACK_TICK_COMP
                | Self::TRACK_GATE_COMP_PERCENT
                | Self::TRACK_KEY_COMP
                | Self::TRACK_ONIONSKIN_COLOR_INDEX
                | Self::TRACK_TICK_COMP_FROM_MEA
                | Self::TRACK_NOTE_RANGE_L
                | Self::TRACK_NOTE_RANGE_H
                | Self::ABS_TICK_POS
                | Self::NOTE_KEY_NUMBER
                | Self::NOTE_VELOCITY
                | Self::NOTE_GATE
                | Self::CONTROL_TYPE
                | Self::MEASURE_LINK_MEASURE
                | Self::MEASURE_LINK_KEY_COMP
                | Self::KEY_SIG_INDEX
                | Self::TIME_SIG_NUMERATOR
                | Self::TIME_SIG_DENOMINATOR
        )
    }

    /// 是否为浮点节点
    pub fn is_float(&self) -> bool {
        matches!(
            *self,
            Self::CONTROL_GATE | Self::CONTROL_VALUE | Self::TEMPO_VALUE
        )
    }

    /// 获取基础类型 ID（低 16 位）
    pub fn base_type(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// 获取完整类型 ID
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u16> for DmsNodeType {
    fn from(value: u16) -> Self {
        Self(value as u64)
    }
}

impl From<u64> for DmsNodeType {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
