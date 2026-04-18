use super::DmsNodeType;

impl DmsNodeType {
    /// 从原始类型 ID 和父节点构建完整类型
    #[must_use]
    pub fn from_parts(type_id: u16, _layer: i32, parent: Option<&DmsNodeType>) -> Self {
        if let Some(parent) = parent {
            let result = u64::from(type_id) | (parent.0 << 16);
            let parent_high = (result & 0x0000_0000_FFFF_0000) >> 16;
            let parent_low = result & 0xFFFF_FFFF_0000_FFFF;
            if parent_high >= 2000 && parent_low == Self::ABS_TICK_POS.0 & 0xFFFF_FFFF_0000_FFFF {
                return Self::ABS_TICK_POS;
            }
            Self(result)
        } else {
            Self(u64::from(type_id))
        }
    }

    /// 是否为复合节点
    #[must_use]
    pub const fn is_composite(&self) -> bool {
        let base = self.base_type();
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
        ) || (self.0 >> 16 == Self::TRACK.0 && base >= 2001 && base <= 2019)
    }

    /// 是否为字符串节点
    #[must_use]
    pub const fn is_string(&self) -> bool {
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
    #[must_use]
    pub const fn is_integer(&self) -> bool {
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
    #[must_use]
    pub const fn is_float(&self) -> bool {
        matches!(
            *self,
            Self::CONTROL_GATE | Self::CONTROL_VALUE | Self::TEMPO_VALUE
        )
    }

    /// 获取基础类型 ID（低 16 位）
    #[must_use]
    pub const fn base_type(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// 获取完整类型 ID
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}
