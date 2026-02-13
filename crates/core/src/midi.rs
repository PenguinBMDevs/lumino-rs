pub mod loader;

use std::path::PathBuf;

// ============================================================================
// MIDI 数据结构
// ============================================================================

/// 解析后的MIDI数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedMidi {
    pub info: MidiInfo,
    /// MIDI文件原始数据，仅在加载后临时存在，使用 `take_midi_data` 取出后会变为 None
    /// 避免长时间占用内存
    #[serde(skip)]
    pub midi_data: Option<Vec<u8>>,
}

impl ParsedMidi {
    /// 取出MIDI原始数据（用于保存LMPJ），取出后内存会被释放
    pub fn take_midi_data(&mut self) -> Option<Vec<u8>> {
        self.midi_data.take()
    }
}

/// MIDI文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiInfo {
    pub path: PathBuf,
    pub track_count: u16,
    pub total_notes: u64,
    pub duration_ticks: u32,
    pub division: u16,
    pub parse_progress: Option<f64>,
}

impl MidiInfo {
    /// 解析MIDI文件
    pub fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None)
    }

    /// 解析MIDI文件（带进度回调）
    ///
    /// `progress_callback` 接收 0.0..=100.0 的百分比值。
    pub fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
        loader::load_midi_info_with_progress(path, progress_callback)
    }
}

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MIDI文件: {}\n音轨数: {}\n音符事件数: {}\n时长: {} ticks\n分辨率: {}",
            self.path.display(),
            self.track_count,
            self.total_notes,
            self.duration_ticks,
            self.division,
        )
    }
}

// ============================================================================
// DMS 数据结构
// ============================================================================

/// 解析后的 DMS 数据（轻量级）
#[derive(Debug)]
pub struct ParsedDms {
    /// DMS 文件信息
    pub info: DmsInfo,
    /// 轻量级数据（零拷贝引用），流式扫描时为None
    data: Option<lumino_dms::DmsLightweightData>,
}

impl ParsedDms {
    /// 延迟解析完整节点树（用于需要编辑时）
    pub fn parse_full(&self) -> Result<lumino_dms::DmsCompositeNode, String> {
        self.data
            .as_ref()
            .ok_or_else(|| "需要加载完整DMS数据才能解析".to_string())?
            .parse_full()
            .map_err(|e| format!("解析 DMS 节点树失败: {e}"))
    }

    /// 获取原始数据大小
    pub fn data_size(&self) -> usize {
        self.data.as_ref().map(|d| d.len()).unwrap_or(0)
    }
}

/// DMS 文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DmsInfo {
    /// 文件路径
    pub path: PathBuf,
    /// 歌曲名称
    pub song_name: Option<String>,
    /// 歌曲版权信息
    pub copyright: Option<String>,
    /// 歌曲备注
    pub comment: Option<String>,
    /// PPQN (每四分音符的脉冲数)
    pub ppqn: Option<u32>,
    /// 轨道数量
    pub track_count: usize,
    /// 总音符数
    pub total_notes: u64,
    /// 工作时间（秒）
    pub working_time_sec: Option<u64>,
}

impl std::fmt::Display for DmsInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DMS 文件: {}", self.path.display())?;
        if let Some(ref name) = self.song_name {
            writeln!(f, "歌曲名称: {}", name)?;
        }
        if let Some(ref copyright) = self.copyright {
            writeln!(f, "版权信息: {}", copyright)?;
        }
        if let Some(ppqn) = self.ppqn {
            writeln!(f, "PPQN: {}", ppqn)?;
        }
        writeln!(f, "轨道数量: {}", self.track_count)?;
        writeln!(f, "音符总数: {}", self.total_notes)?;
        if let Some(time) = self.working_time_sec {
            let mins = time / 60;
            let secs = time % 60;
            writeln!(f, "工作时间: {}分{}秒", mins, secs)?;
        }
        Ok(())
    }
}
