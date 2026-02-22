use std::path::PathBuf;

/// DMS 文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DmsInfo {
    pub path: PathBuf,
    pub song_name: Option<String>,
    pub copyright: Option<String>,
    pub comment: Option<String>,
    pub ppqn: Option<u32>,
    pub track_count: usize,
    pub total_notes: u64,
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
