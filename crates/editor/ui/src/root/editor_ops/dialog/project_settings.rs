//! 工程设置对话框数据管理

use crate::root::Root;

/// 工程设置对话框数据包（消除 `set_project_settings_data` 的 8 个参数）
#[derive(Debug, Clone, Default)]
pub struct ProjectSettingsDialogData {
    /// 项目标题
    pub title: String,
    /// 速度（BPM）
    pub tempo: String,
    /// 版权信息
    pub copyright: String,
    /// 作者
    pub author: String,
    /// 创建时间显示文本
    pub created_display: String,
    /// 总编辑时长（秒）
    pub total_editing_time_seconds: f64,
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
}

impl Root {
    /// 重置工程设置对话框状态到默认值。
    ///
    /// 工程设置（标题/作者/版权/BPM/拍号）属于工程级数据，关闭工程、
    /// 新建工程或加载新文件时必须重置，防止旧工程数据残留在
    /// 程序全局状态中（修复：关闭工程后工程设置面板仍显示旧工程数据）。
    pub fn reset_project_settings(&mut self) {
        self.state.project_settings_dialog.reset();
    }

    /// 设置工程设置对话框数据
    pub fn set_project_settings_data(&mut self, data: ProjectSettingsDialogData) {
        self.state.project_settings_dialog.title = data.title;
        self.state.project_settings_dialog.tempo = data.tempo;
        self.state.project_settings_dialog.copyright = data.copyright;
        self.state.project_settings_dialog.author = data.author;
        self.state.project_settings_dialog.created_display = data.created_display;
        self.state
            .project_settings_dialog
            .total_editing_time_seconds = data.total_editing_time_seconds;
        let (numerator, denominator) = data
            .time_signatures
            .first()
            .map(|(_, n, d)| (*n, *d))
            .unwrap_or((4, 4));
        self.state.project_settings_dialog.time_signature_numerator = numerator.to_string();
        self.state
            .project_settings_dialog
            .time_signature_denominator = denominator.to_string();
    }

    /// 应用工程设置到主窗口
    pub fn apply_project_settings(
        &mut self,
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
    ) {
        tracing::info!(
            "应用工程设置: 标题={}, BPM={}, 版权={}, 作者={}, 拍号变化数={}",
            title,
            tempo,
            copyright,
            author,
            time_signatures.len()
        );

        // 持久化标题、版权和作者
        self.state.project_settings_dialog.title = title.clone();
        self.state.project_settings_dialog.copyright = copyright;
        self.state.project_settings_dialog.author = author;
        self.state.project_settings_dialog.tempo = format!("{:.0}", tempo);

        // 同步到编辑器 tempo 数据（用户编辑的源，同时同步 document.tempo_changes）
        self.editor.editor_state.data.set_tempo_points(vec![
            crate::editor::editor_state::TempoPoint {
                tick: 0.0,
                bpm: tempo,
            },
        ]);

        // 同步拍号变化到编辑器数据（经统一入口，同时同步 document.time_signatures）
        let mut sorted_ts = time_signatures;
        sorted_ts.sort_by_key(|(tick, _, _)| *tick);
        if sorted_ts.is_empty() {
            sorted_ts.push((0, 4, 4));
        }
        self.editor.editor_state.data.set_time_signatures(sorted_ts);
        // 拍号变化可能影响网格，清空相关缓存
        self.editor.grid_cache.clear();
        self.editor.ruler_cache.clear();

        // 同步到播放管理器
        let tempo_micros = lumino_midi_loader::bpm_to_tempo(tempo) as u32;
        self.load_tempo_changes(vec![(0, tempo_micros)]);
    }

    /// 获取当前项目设置数据（用于填充工程设置对话框）
    /// 返回 (title, tempo, copyright, author, created_display, total_editing_time_seconds, time_signatures)
    #[allow(clippy::type_complexity)]
    pub fn get_project_settings_data(
        &self,
    ) -> (
        String,
        String,
        String,
        String,
        String,
        f64,
        Vec<(u32, u8, u8)>,
    ) {
        let dialog = &self.state.project_settings_dialog;
        // 从编辑器 tempo_points 读取当前 BPM（反映工程设置和指挥轨道编辑的变更）
        let tempo = self
            .editor
            .editor_state
            .data
            .tempo_points
            .first()
            .map(|tp| format!("{:.1}", tp.bpm))
            .unwrap_or_else(|| dialog.tempo.clone());
        let created_display = dialog.created_display.clone();
        let editing_time = dialog.total_editing_time_seconds;
        let time_signatures = self.editor.editor_state.data.time_signatures.clone();

        // 从 MIDI 文档获取标题和版权（如果有）
        // 2026-08 单一权威源：以 EditorData.document 是否存在判断（midi_state 不再持有）。
        let (title, copyright) = if self.editor.editor_state.data.document.is_some() {
            // 尝试从文件名获取标题
            let title = if dialog.title.is_empty() {
                // 使用默认标题
                "无标题".to_string()
            } else {
                dialog.title.clone()
            };
            (title, dialog.copyright.clone())
        } else {
            (dialog.title.clone(), dialog.copyright.clone())
        };

        (
            title,
            tempo,
            copyright,
            dialog.author.clone(),
            created_display,
            editing_time,
            time_signatures,
        )
    }
}
