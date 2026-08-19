//! MIDI 协议常量定义

// 通道相关
/// MIDI 通道数量
pub const MIDI_CHANNEL_COUNT: u8 = 16;
/// 通道号掩码（低 4 位）
pub const MIDI_CHANNEL_MASK: u8 = 0x0F;

// 数据范围
/// MIDI 数据值掩码（低 7 位）
pub const MIDI_VALUE_MASK: u8 = 0x7F;
/// MIDI 数据值最大值
pub const MIDI_VALUE_MAX: u8 = 127;
/// 弯音最大值
pub const PITCH_BEND_MAX: u16 = 16383;

// 状态字节
/// Note On 状态字节基值
pub const STATUS_NOTE_ON: u8 = 0x90;
/// Note Off 状态字节基值
pub const STATUS_NOTE_OFF: u8 = 0x80;
/// 控制器变化（CC）状态字节基值
pub const STATUS_CONTROL_CHANGE: u8 = 0xB0;
/// 音色变换（Program Change）状态字节基值
pub const STATUS_PROGRAM_CHANGE: u8 = 0xC0;
/// 弯音（Pitch Bend）状态字节基值
pub const STATUS_PITCH_BEND: u8 = 0xE0;
/// 通道后触状态字节基值
pub const STATUS_CHANNEL_PRESSURE: u8 = 0xD0;
/// 复音后触状态字节基值
pub const STATUS_POLY_PRESSURE: u8 = 0xA0;

// 控制器编号
/// CC 123：所有音符关闭（All Notes Off）
pub const CC_ALL_NOTES_OFF: u8 = 123;
/// CC 121：重置所有控制器
pub const CC_RESET_ALL_CONTROLLERS: u8 = 121;

// 默认值
/// 默认采样率
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;
