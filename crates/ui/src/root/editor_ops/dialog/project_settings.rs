//! 工程设置对话框数据管理

use crate::root::Root;

impl Root {
    /// 设置工程设置对话框数据
    pub fn set_project_settings_data(
        &mut self,
        title: String,
        tempo: String,
        copyright: String,
        author: String,
        created_display: String,
        total_editing_time_seconds: f64,
        time_signatures: Vec<(u32, u8, u8)>,
    ) {
        self.state.project_settings_dialog.title = title;
        self.state.project_settings_dialog.tempo = tempo;
        self.state.project_settings_dialog.copyright = copyright;
        self.state.project_settings_dialog.author = author;
        self.state.project_settings_dialog.created_display = created_display;
        self.state
            .project_settings_dialog
            .total_editing_time_seconds = total_editing_time_seconds;
        let (numerator, denominator) = time_signatures
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

        // 同步到编辑器 tempo 数据（用户编辑的源）
        self.editor.editor_state.data.tempo_points =
            vec![crate::editor::editor_state::TempoPoint {
                tick: 0.0,
                bpm: tempo,
            }];

        // 同步拍号变化到编辑器数据
        let mut sorted_ts = time_signatures;
        sorted_ts.sort_by_key(|(tick, _, _)| *tick);
        if sorted_ts.is_empty() {
            sorted_ts.push((0, 4, 4));
        }
        self.editor.editor_state.data.time_signatures = sorted_ts;
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
