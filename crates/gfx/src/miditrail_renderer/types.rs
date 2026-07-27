//! Miditrail 3D 渲染器共享类型
//!
//! 用于视频导出的 3D MIDI 轨迹可视化风格，
//! 参考经典 3D MIDI 轨迹渲染的相机、键盘与音符布局，但实现于 wgpu 渲染管线中。

/// 单个音符的 CPU 侧数据（与 `GpuVisibleNote` 对应）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MiditrailNoteGpu {
    /// MIDI 键号（0-127）
    pub key: u32,
    /// 音符起始 tick
    pub start_tick: u32,
    /// 音符结束 tick
    pub end_tick: u32,
    /// 打包 RGBA 颜色（`0xRRGGBBAA`）
    pub color_packed: u32,
    /// 音轨索引（用于调色板）
    pub track_idx: u32,
    /// 音符力度（0-127）
    pub velocity: u32,
    /// MIDI 通道（0-15）
    pub channel: u32,
    /// 对齐填充
    pub _padding: u32,
}

impl MiditrailNoteGpu {
    /// 判断该音符在当前 tick 是否处于激活状态（按下并持续）
    #[must_use]
    pub fn is_active_at(&self, tick: u32) -> bool {
        self.start_tick <= tick && self.end_tick > tick
    }

    /// 判断该音符在当前 tick 是否仍可见（尚未结束）
    #[must_use]
    pub fn is_visible_at(&self, tick: u32) -> bool {
        self.end_tick > tick
    }
}

/// 每帧渲染参数（CPU 侧使用，不直接上传 GPU）
#[derive(Debug, Clone, Copy)]
pub struct MiditrailUniformGpu {
    /// 当前播放 tick
    pub tick: u32,
    /// 每四分音符 tick 数
    pub ppq: u32,
    /// 键盘键数（固定 128）
    pub key_count: u32,
    /// 帧宽度（像素）
    pub frame_width: u32,
    /// 帧高度（像素）
    pub frame_height: u32,
    /// 键盘高度（像素，占位）
    pub kb_height: u32,
    /// 保留
    pub _reserved: u32,
    /// 滚动速度倍率
    pub speed: f32,
    /// 样式附加参数 1
    pub param1: f32,
    /// 样式附加参数 2
    pub param2: f32,
    /// 对齐填充
    pub _padding0: u32,
    pub _padding1: u32,
}

impl Default for MiditrailUniformGpu {
    fn default() -> Self {
        Self {
            tick: 0,
            ppq: 480,
            key_count: 128,
            frame_width: 1920,
            frame_height: 1080,
            kb_height: 128,
            _reserved: 0,
            speed: 1.0,
            param1: 0.0,
            param2: 0.0,
            _padding0: 0,
            _padding1: 0,
        }
    }
}

/// 相机 Uniform（上传 GPU）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MiditrailCameraGpu {
    /// 投影 * 视图矩阵（column-major）
    pub view_proj: [[f32; 4]; 4],
    /// 指向光源的方向（已归一化）
    pub light_dir: [f32; 3],
    /// 环境光强度
    pub ambient: f32,
}

/// 每实例数据（上传 GPU，与 WGSL 中的 `Instance` 对应）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MiditrailInstanceGpu {
    /// 模型平移
    pub translation: [f32; 3],
    pub _padding0: f32,
    /// 模型缩放（决定立方体大小）
    pub scale: [f32; 3],
    pub _padding1: f32,
    /// 打包 RGBA 颜色
    pub color_packed: u32,
    /// 1 表示琴键，0 表示音符
    pub is_key: u32,
    /// 对齐填充到 16 字节边界
    pub _padding2: [u32; 2],
}

impl MiditrailInstanceGpu {
    /// 创建新的实例数据
    #[must_use]
    pub fn new(translation: [f32; 3], scale: [f32; 3], color_packed: u32, is_key: bool) -> Self {
        Self {
            translation,
            _padding0: 0.0,
            scale,
            _padding1: 0.0,
            color_packed,
            is_key: u32::from(is_key),
            _padding2: [0; 2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_miditrail_note_active() {
        let note = MiditrailNoteGpu {
            key: 60,
            start_tick: 100,
            end_tick: 200,
            color_packed: 0,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        };
        assert!(note.is_active_at(150));
        assert!(!note.is_active_at(99));
        assert!(!note.is_active_at(200));
        assert!(note.is_visible_at(150));
        assert!(!note.is_visible_at(250));
    }

    #[test]
    fn test_instance_size() {
        assert_eq!(std::mem::size_of::<MiditrailInstanceGpu>(), 48);
        assert_eq!(std::mem::size_of::<MiditrailCameraGpu>(), 80);
    }
}
