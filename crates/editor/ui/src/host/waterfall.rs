//! 贴图瀑布流 —— 从 Host 拆出的贴图瀑布流相关方法
//!
//! 管理贴图瀑布流的生成、重生成、脏区域追踪等。

use super::Host;

impl Host {
    /// 启动贴图瀑布流生成（播放器音符显示用）
    pub fn generate_texture_waterfall(
        &mut self,
        notes: Vec<Vec<lumino_gfx::WaterfallNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        config: lumino_gfx::TextureWaterfallConfig,
        midi_hash: String,
    ) {
        // 存下上下文供重生成使用
        self.waterfall_midi_hash = Some(midi_hash.clone());
        self.waterfall_gen_info = Some((ppq, key_count, total_ticks));
        self.waterfall_config = Some(config.clone());
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::Waterfall(
                lumino_gfx::WaterfallCommand::Generate {
                    notes,
                    ppq,
                    key_count,
                    total_ticks,
                    config,
                    midi_hash,
                },
            ));
        }
    }

    /// 释放贴图瀑布流资源（关闭 MIDI / 新建工程时调用）
    ///
    /// 释放 GPU 资源后，根据当前编辑器视图状态重新初始化默认上下文，
    /// 保证干净启动或关闭文件后仍能继续编辑并生成贴图。
    pub fn dispose_texture_waterfall(&mut self) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::Waterfall(
                lumino_gfx::WaterfallCommand::Dispose,
            ));
        }
        self.waterfall_dirty_tracks.clear();
        self.init_default_waterfall_context();
    }

    /// 根据当前编辑器视图状态初始化默认高精度上下文
    ///
    /// 无 MIDI 文件时（干净启动 / 新建工程）使用 editor 的默认 ppq/key_count/total_ticks。
    pub(super) fn init_default_waterfall_context(&mut self) {
        let view = &self.root.editor.editor_state.view;
        let key_count = view.key_count;
        let ppq = view.ppq;
        let total_ticks = view.total_ticks;
        let ui_cfg = self.waterfall_config.clone().unwrap_or_else(|| {
            let default = lumino_gfx::TextureWaterfallConfig::default();
            lumino_gfx::TextureWaterfallConfig {
                enabled: default.enabled,
                measures_per_group: default.measures_per_group,
                tile_width_px: default.tile_width_px,
                cooldown_secs: default.cooldown_secs,
                gpu_mem_limit_mb: default.gpu_mem_limit_mb,
                group_tile_mem_limit_mb: default.group_tile_mem_limit_mb,
                render_mode: default.render_mode,
                cache_dir: default.cache_dir,
            }
        });
        let config = lumino_gfx::TextureWaterfallConfig {
            enabled: ui_cfg.enabled,
            measures_per_group: ui_cfg.measures_per_group,
            tile_width_px: ui_cfg.tile_width_px,
            cooldown_secs: ui_cfg.cooldown_secs,
            gpu_mem_limit_mb: ui_cfg.gpu_mem_limit_mb,
            group_tile_mem_limit_mb: ui_cfg.group_tile_mem_limit_mb,
            render_mode: ui_cfg.render_mode,
            cache_dir: ui_cfg.cache_dir,
        };
        let midi_hash = lumino_gfx::compute_waterfall_cache_hash(b"empty-project");
        self.waterfall_config = Some(config);
        self.waterfall_midi_hash = Some(midi_hash);
        self.waterfall_gen_info = Some((ppq, key_count, total_ticks));
    }

    /// 发送贴图瀑布流重生命令（冷静期到期后由 runner 调用）
    ///
    /// `group_notes` 需包含该 `track_idx` 所在 track_group 的所有音轨音符，
    /// runner 将使用这些最新数据重新合并 group tile，避免读取过期缓存。
    pub fn send_waterfall_regen(&mut self, params: lumino_gfx::WaterfallTrackParams) {
        self.send_waterfall_track_cmd(lumino_gfx::render_thread::ControlCommand::regenerate_track(
            params,
        ));
    }

    /// 内部：发送高精度音轨相关控制命令
    fn send_waterfall_track_cmd(&mut self, cmd: lumino_gfx::render_thread::ControlCommand) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(cmd);
        }
    }

    /// 获取贴图瀑布流生成时的 MIDI 哈希（供 runner 冷静期检查使用）
    pub fn waterfall_midi_hash(&self) -> Option<&str> {
        self.waterfall_midi_hash.as_deref()
    }

    /// 获取贴图瀑布流生成时的 (ppq, key_count, total_ticks)（供 runner 冷静期检查使用）
    pub fn waterfall_gen_info(&self) -> Option<(u16, u16, u32)> {
        self.waterfall_gen_info
    }

    /// 标记当前音轨贴图瀑布流为脏（音符编辑后调用）
    ///
    /// 2026-08-06 性能修复：此前在此全轨克隆音符快照（`get_track_notes_for_waterfall`
    /// 1600W 工程 ≈ 320MB Vec）+ 全量计算 time_group HashSet（~780ms/次），
    /// 而产物（`waterfall_dirty_regions` / `waterfall_dirty_time_groups`）全仓**只写不读**
    /// ——`force_waterfall_regen` 重生成时实时从 document 收集，从不消费快照。
    /// 死数据已删除，此处仅保留 O(1) 脏标记（供未来按需重生成接线使用）。
    pub fn mark_waterfall_dirty(&mut self, track_idx: u16) {
        self.waterfall_dirty_tracks.insert(track_idx);
    }

    /// 获取贴图瀑布流配置引用（供 runner 构建重生成上下文时使用）
    pub fn waterfall_config_ref(&self) -> Option<&lumino_gfx::TextureWaterfallConfig> {
        self.waterfall_config.as_ref()
    }

    /// 立即触发脏音轨重生成（绕过冷静期）
    ///
    /// 在以下场景调用：
    /// - 用户从脏音轨切换到其他音轨
    /// - 需要在渲染线程开始后台重生，生成的贴图通过流式通道传回 GPU 上传
    ///
    /// 仅在 `waterfall_dirty_tracks` 包含该音轨且配置信息完整时生效。
    /// 调用后会从脏集合中移除该音轨。
    ///
    /// 重生成以音轨组为单位，使用整个 track_group 的最新音符数据，
    /// 避免同组其他音轨被覆盖为旧数据或空数据。
    pub fn force_waterfall_regen(&mut self, track_idx: u16) {
        if !self.waterfall_dirty_tracks.remove(&track_idx) {
            return; // 该音轨不脏，不触发
        }

        let Some(cfg) = self.waterfall_config.clone() else {
            return;
        };
        let Some(hash) = self.waterfall_midi_hash.clone() else {
            return;
        };
        let Some((ppq, key_count, total_ticks)) = self.waterfall_gen_info else {
            return;
        };

        // 音轨总数：取当前侧边栏音轨数与脏音轨索引+1 的较大值，
        // 确保干净启动时也能正确推断音轨组范围。
        let track_count = (self.root.sidebar.tracks.len() as u16).max(track_idx + 1);
        let group_notes = self.collect_waterfall_group_notes(track_idx, track_count);

        self.send_waterfall_regen(lumino_gfx::WaterfallTrackParams {
            track_idx,
            group_notes,
            // 重生命令做全量替换，不需要按 time_group 过滤
            dirty_time_groups: Vec::new(),
            ppq,
            key_count,
            total_ticks,
            track_count,
            config: cfg,
            midi_hash: hash,
        });
    }

    /// 收集指定音轨所在 track_group 的所有音轨音符
    ///
    /// 返回的 Vec 索引 0 对应该 track_group 的第一个音轨。
    fn collect_waterfall_group_notes(
        &self,
        track_idx: u16,
        track_count: u16,
    ) -> Vec<Vec<lumino_gfx::WaterfallNote>> {
        let track_group = (track_idx / lumino_gfx::WATERFALL_TRACKS_PER_GROUP) as u32;
        let track_start = (track_group * lumino_gfx::WATERFALL_TRACKS_PER_GROUP as u32) as u16;
        let track_end = (track_start + lumino_gfx::WATERFALL_TRACKS_PER_GROUP).min(track_count);
        (track_start..track_end)
            .map(|t| self.get_track_notes_for_waterfall(t))
            .collect()
    }

    /// 获取指定音轨的音符列表（用于贴图瀑布流重生成）
    ///
    /// 2026-08 单一权威源：一律从 document 读取（track_notes 缓存已删除）。
    pub fn get_track_notes_for_waterfall(&self, track_idx: u16) -> Vec<lumino_gfx::WaterfallNote> {
        let editor = &self.root.editor;
        editor
            .editor_state
            .data
            .track_notes(track_idx as usize)
            .iter()
            .map(|ne| {
                lumino_gfx::WaterfallNote::from_ms(
                    ne.start_tick as f32,
                    ne.end_tick as f32,
                    ne.key,
                    super::onion_track_color(track_idx as usize),
                )
            })
            .collect()
    }

    /// 取出贴图瀑布流生成进度（runner 每帧调用并转发到进度窗口）
    pub fn drain_waterfall_progress(&self) -> Vec<(String, f32)> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.drain_waterfall_progress())
            .unwrap_or_default()
    }
}
