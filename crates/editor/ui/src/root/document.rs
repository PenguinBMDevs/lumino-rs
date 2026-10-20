//! Root MIDI 文档挂载
//!
//! 设置 MIDI 文档（独占所有权，2026-08 单一权威源）并重建自动化 lane。

use crate::root::Root;
use lumino_midi_loader::MidiDocument;

impl Root {
    /// 设置 MIDI 文档（独占所有权，供编辑/渲染/保存）
    ///
    /// 2026-08 单一权威源改造：`EditorData.document` 独占持有 `MidiDocument`，
    /// 不再以 `Arc` 共享。控制事件按音轨导入 automation_lanes（与 Yinhe 对齐）。
    pub fn set_midi_document(&mut self, doc: MidiDocument) {
        // REND-003：批量导入控制事件（CC/PB → automation lane）。
        // 旧实现逐条 `apply_automation_edit`：每条 retain + 全量 sort + 重算控制柄，
        // 单 lane 累计 O(M² log M)——实测 32.3 万条事件的 lane 会让加载长时间卡死；
        // 批量路径按 (track, target) 分组单遍构建，语义见方法文档。
        self.editor
            .editor_state
            .data
            .import_control_events_from_document(&doc);

        // REND-002 修复：真正装载新文档时清零打击乐模态跟踪。
        // `update_playback_notes` 里的 `set_document` 同时服务编辑快照（必须保留
        // 运行时 Bank Select 推导出的模态），故「切换」信号必须由这里显式给出。
        // 不清零会让 msb_seen / bank_msb 跨文档泄漏：新文档只发 CC32（无 CC0）时
        // 会被旧文档残留的证据误判为需要切换模态。
        if let Some(manager) = &mut self.playback.manager {
            manager.reset_percussion_tracking();
        }

        // 单一权威源：文档独占存入 EditorData
        self.editor.editor_state.data.document = Some(doc);
        // 装载后刷新 tempo / 拍号的编辑态缓存（2026-10-20 审计 §三）：
        // 此前由调用方（`host::editor_ops::set_midi_document`）「从 doc 深拷贝 →
        // set_tempo_points 写回 doc」，是一次毫无意义的往返；现在统一在「文档进入
        // 编辑器」这一处只读刷新，且不置脏、不回写。
        self.editor
            .editor_state
            .data
            .adopt_tempo_cache_from_document();
        // 装载新文档 = 音符内容变化：必须 bump `track_notes_gen`。
        //
        // 该世代是所有音符派生缓存的失效键（洋葱皮、走带 max_tick、播放键色索引
        // `PlaybackScanState.notes_gen`…）。此前只有
        // `host::editor_ops::set_midi_document` 会 bump，直接调用本方法的路径
        // （测试、协作/素材装载等）装载后世代不变，派生缓存会继续用上一份文档的
        // 内容——播放键色索引此前靠文档内存地址兜底，而地址在旧文档 drop 后
        // 可能被新文档复用（见审计 §五）。把不变量放在「文档进入编辑器」这一处。
        self.editor.editor_state.data.mark_track_notes_changed();
    }
}
