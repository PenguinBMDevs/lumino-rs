//! MIDI 协议常量定义

// 通道相关
pub const MIDI_CHANNEL_COUNT: u8 = 16;
pub const MIDI_CHANNEL_MASK: u8 = 0x0F;

// 数据范围
pub const MIDI_VALUE_MASK: u8 = 0x7F;
pub const MIDI_VALUE_MAX: u8 = 127;
pub const PITCH_BEND_MAX: u16 = 16383;

// 状态字节
pub const STATUS_NOTE_ON: u8 = 0x90;
pub const STATUS_NOTE_OFF: u8 = 0x80;
pub const STATUS_CONTROL_CHANGE: u8 = 0xB0;
pub const STATUS_PROGRAM_CHANGE: u8 = 0xC0;
pub const STATUS_PITCH_BEND: u8 = 0xE0;
pub const STATUS_CHANNEL_PRESSURE: u8 = 0xD0;
pub const STATUS_POLY_PRESSURE: u8 = 0xA0;

// 控制器编号
pub const CC_ALL_NOTES_OFF: u8 = 123;
pub const CC_RESET_ALL_CONTROLLERS: u8 = 121;

// 默认值
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;
