//! 高精度贴图 —— 从 Host 拆出的高精度洋葱皮贴图相关方法
//!
//! 管理高精度贴图的生成、重生成、脏区域追踪、冷静期控制等。

use std::time::Instant;

use super::Host;

impl Host {
    /// 启动高精度洋葱皮贴图生成（MIDI 加载后调用）
    pub fn generate_hires_onion_skin(
        &mut self,
        notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        config: lumino_gfx::HiResConfig,
        midi_hash: String,
    ) {
        // 存下上下文供重生成使用
        self.hires_midi_hash = Some(midi_hash.clone());
        self.hires_gen_info = Some((ppq, key_count, total_ticks));
        self.hires_config = Some(config.clone());
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::GenerateHiResOnionSkin {
                    notes,
                    ppq,
                    key_count,
                    total_ticks,
                    config,
                    midi_hash,
                },
            );
        }
    }

    /// 释放高精度洋葱皮资源（关闭 MIDI / 新建工程时调用）
    ///
    /// 释放 GPU 资源后，根据当前编辑器视图状态重新初始化默认上下文，
    /// 保证干净启动或关闭文件后仍能继续编辑并生成贴图。
    pub fn dispose_hires_onion_skin(&mut self) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::DisposeHiResOnionSkin);
        }
        self.hires_dirty_tracks.clear();
        self.hires_dirty_regions.clear();
        self.hires_last_edit = None;
        self.init_default_hires_context();
    }

    /// 根据当前编辑器视图状态初始化默认高精度洋葱皮上下文
    ///
    /// 无 MIDI 文件时（干净启动 / 新建工程）使用 editor 的默认 ppq/key_count/total_ticks。
    pub(super) fn init_default_hires_context(&mut self) {
        let view = &self.root.editor.editor_state.view;
        let key_count = view.key_count;
        let ppq = view.ppq;
        let total_ticks = view.total_ticks;
        let ui_cfg = self.hires_config.clone().unwrap_or_else(|| {
            let default = lumino_gfx::HiResConfig::default();
            lumino_gfx::HiResConfig {
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
        let config = lumino_gfx::HiResConfig {
            enabled: ui_cfg.enabled,
            measures_per_group: ui_cfg.measures_per_group,
            tile_width_px: ui_cfg.tile_width_px,
            cooldown_secs: ui_cfg.cooldown_secs,
            gpu_mem_limit_mb: ui_cfg.gpu_mem_limit_mb,
            group_tile_mem_limit_mb: ui_cfg.group_tile_mem_limit_mb,
            render_mode: ui_cfg.render_mode,
            cache_dir: ui_cfg.cache_dir,
        };
        let midi_hash = lumino_gfx::compute_midi_hash(b"empty-project");
        self.hires_config = Some(config);
        self.hires_midi_hash = Some(midi_hash);
        self.hires_gen_info = Some((ppq, key_count, total_ticks));
    }

    /// 发送高精度贴图重生命令（冷静期到期后由 runner 调用）
    ///
    /// `group_notes` 需包含该 `track_idx` 所在 track_group 的所有音轨音符，
    /// runner 将使用这些最新数据重新合并 group tile，避免读取过期缓存。
    pub fn send_hires_regen(&mut self, params: lumino_gfx::render_thread::HiResTrackParams) {
        self.send_hires_track_cmd(lumino_gfx::render_thread::ControlCommand::regenerate_track(
            params,
        ));
    }

    /// 发送编辑后的临时脏区域覆层显示命令（切换音轨前立即触发）
    pub fn send_hires_dirty_overlay(
        &mut self,
        params: lumino_gfx::render_thread::HiResTrackParams,
    ) {
        self.send_hires_track_cmd(
            lumino_gfx::render_thread::ControlCommand::show_dirty_overlay(params),
        );
    }

    /// 内部：发送高精度音轨相关控制命令
    fn send_hires_track_cmd(&mut self, cmd: lumino_gfx::render_thread::ControlCommand) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(cmd);
        }
    }

    /// 获取高精度贴图生成时的 MIDI 哈希（供 runner 冷静期检查使用）
    pub fn hires_midi_hash(&self) -> Option<&str> {
        self.hires_midi_hash.as_deref()
    }

    /// 获取高精度贴图生成时的 (ppq, key_count, total_ticks)（供 runner 冷静期检查使用）
    pub fn hires_gen_info(&self) -> Option<(u16, u16, u32)> {
        self.hires_gen_info
    }

    /// 标记当前音轨高精度贴图为脏（音符编辑后调用）
    ///
    /// 同时收集该音轨的脏区域音符快照，用于生成临时贴图覆层。
    pub fn mark_hires_dirty(&mut self, track_idx: u16) {
        self.hires_dirty_tracks.insert(track_idx);
        // 收集当前音轨的所有音符作为脏区域快照
        let notes = self.get_track_notes_for_hires(track_idx);
        tracing::debug!(
            "[onion-dirty] mark_hires_dirty: track={}, notes={}",
            track_idx,
            notes.len()
        );
        self.hires_dirty_regions.insert(track_idx, notes);
        self.hires_last_edit = Some(Instant::now());
        self.hires_overlay_sent = false; // 新脏数据，覆层需重新发送
    }

    /// 将所有当前脏区域作为临时覆层发送到渲染线程
    ///
    /// 在轮询周期调用，让远程编辑的增量变化立即显示为洋葱皮覆层，
    /// 不等冷静期到期或音轨切换。
    ///
    /// **一次性守卫**：同一批脏区域只发送一次覆层到渲染线程，
    /// 防止每帧重复发命令导致渲染线程阻塞。`mark_hires_dirty` 调用后重置。
    ///
    /// 此操作不清理脏标记——覆层只是临时显示，主贴图重生仍需等待冷静期。
    pub fn show_hires_dirty_overlays(&mut self) -> bool {
        if self.hires_dirty_regions.is_empty() {
            tracing::debug!("[onion-dirty] show_hires_dirty_overlays: 无脏区域");
            return false;
        }
        if self.hires_overlay_sent {
            tracing::debug!("[onion-dirty] show_hires_dirty_overlays: 覆层已发送，跳过");
            return false;
        }
        let Some((ppq, key_count, total_ticks)) = self.hires_gen_info else {
            tracing::warn!("[onion-dirty] show_hires_dirty_overlays: hires_gen_info 为空");
            return false;
        };
        let Some(config) = self.hires_config.clone() else {
            tracing::warn!("[onion-dirty] show_hires_dirty_overlays: hires_config 为空");
            return false;
        };
        let track_count = self.track_count() as u16;

        // 先收集所有脏音轨的快照数据，避免迭代时同时 borrow self
        let dirty_snapshots: Vec<(u16, Vec<lumino_gfx::OnionSkinNote>)> = self
            .hires_dirty_regions
            .iter()
            .map(|(&t, n)| (t, n.clone()))
            .collect();

        for (track_idx, notes) in &dirty_snapshots {
            let track_group = track_idx / lumino_gfx::TRACKS_PER_GROUP;
            let group_start = track_group * lumino_gfx::TRACKS_PER_GROUP;
            let group_end = (group_start + lumino_gfx::TRACKS_PER_GROUP).min(track_count);
            let mut group_notes: Vec<Vec<lumino_gfx::OnionSkinNote>> = Vec::new();
            for t in group_start..group_end {
                if t == *track_idx {
                    group_notes.push(notes.clone());
                } else {
                    group_notes.push(self.get_track_notes_for_hires(t));
                }
            }

            self.send_hires_dirty_overlay(lumino_gfx::render_thread::HiResTrackParams {
                track_idx: *track_idx,
                group_notes,
                ppq,
                key_count,
                total_ticks,
                track_count,
                config: config.clone(),
                midi_hash: self.hires_midi_hash.clone().unwrap_or_default(),
            });
        }
        self.hires_overlay_sent = true;
        true
    }

    /// 检查冷静期是否到期，返回需要重生成的脏音轨列表
    pub fn check_hires_regen(&mut self) -> Option<Vec<u16>> {
        if self.hires_dirty_tracks.is_empty() {
            return None;
        }
        let cooldown = self
            .hires_config
            .as_ref()
            .map(|c| c.cooldown_secs)
            .unwrap_or(crate::constants::timing::DEFAULT_HIRES_COOLDOWN_SECS);
        if let Some(last) = self.hires_last_edit
            && last.elapsed().as_secs() >= cooldown
        {
            let dirty: Vec<u16> = self.hires_dirty_tracks.iter().copied().collect();
            self.hires_dirty_tracks.clear();
            self.hires_dirty_regions.clear();
            self.hires_last_edit = None;
            self.hires_overlay_sent = false;
            return Some(dirty);
        }
        None
    }

    /// 设置高精度贴图冷静期秒数（从配置初始化）
    pub fn set_hires_cooldown(&mut self, secs: u64) {
        if let Some(ref mut cfg) = self.hires_config {
            cfg.cooldown_secs = secs;
        }
    }

    /// 获取高精度贴图配置引用（供 runner 构建重生成上下文时使用）
    pub fn hires_config_ref(&self) -> Option<&lumino_gfx::HiResConfig> {
        self.hires_config.as_ref()
    }

    /// 立即触发脏音轨重生成（绕过冷静期）
    ///
    /// 在以下场景调用：
    /// - 用户从脏音轨切换到其他音轨
    /// - 需要在渲染线程开始后台重生，生成的贴图通过流式通道传回 GPU 上传
    ///
    /// 仅在 `hires_dirty_tracks` 包含该音轨且配置信息完整时生效。
    /// 调用后会从脏集合中移除该音轨。
    ///
    /// 重生成以音轨组为单位，使用整个 track_group 的最新音符数据，
    /// 避免同组其他音轨被覆盖为旧数据或空数据。
    pub fn force_hires_regen(&mut self, track_idx: u16) {
        tracing::debug!("[onion-dirty] force_hires_regen 进入: track={}", track_idx);
        if !self.hires_dirty_tracks.remove(&track_idx) {
            tracing::debug!(
                "[onion-dirty] force_hires_regen 退出: track={} 不在脏集合",
                track_idx
            );
            return; // 该音轨不脏，不触发
        }
        self.hires_dirty_regions.remove(&track_idx);

        let Some(cfg) = self.hires_config.clone() else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_config 缺失");
            return;
        };
        let Some(hash) = self.hires_midi_hash.clone() else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_midi_hash 缺失");
            return;
        };
        let Some((ppq, key_count, total_ticks)) = self.hires_gen_info else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_gen_info 缺失");
            return;
        };

        // 音轨总数：取当前侧边栏音轨数与脏音轨索引+1 的较大值，
        // 确保干净启动时也能正确推断音轨组范围。
        let track_count = (self.root.sidebar.tracks.len() as u16).max(track_idx + 1);
        let group_notes = self.collect_group_notes(track_idx, track_count);
        tracing::debug!(
            "[onion-dirty] force_hires_regen 发送命令: track={}, group_tracks={}, track_count={}, ppq={}, total_ticks={}",
            track_idx,
            group_notes.len(),
            track_count,
            ppq,
            total_ticks
        );

        self.send_hires_regen(lumino_gfx::render_thread::HiResTrackParams {
            track_idx,
            group_notes,
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
    fn collect_group_notes(
        &self,
        track_idx: u16,
        track_count: u16,
    ) -> Vec<Vec<lumino_gfx::OnionSkinNote>> {
        let track_group = (track_idx / lumino_gfx::TRACKS_PER_GROUP) as u32;
        let track_start = (track_group * lumino_gfx::TRACKS_PER_GROUP as u32) as u16;
        let track_end = (track_start + lumino_gfx::TRACKS_PER_GROUP).min(track_count);
        (track_start..track_end)
            .map(|t| self.get_track_notes_for_hires(t))
            .collect()
    }

    /// 获取指定音轨的音符列表（用于高精度贴图重生成）
    ///
    /// 当前音轨从 editor.notes 取，其他音轨从 track_notes 缓存取。
    pub fn get_track_notes_for_hires(&self, track_idx: u16) -> Vec<lumino_gfx::OnionSkinNote> {
        let editor = &self.root.editor;
        let current_track = editor.current_track();
        let notes = if current_track as u16 == track_idx {
            tracing::debug!(
                "[onion-dirty] get_track_notes_for_hires: track={} 使用当前编辑器音符",
                track_idx
            );
            &editor.editor_state.data.notes
        } else {
            tracing::debug!(
                "[onion-dirty] get_track_notes_for_hires: track={} 使用 track_notes 缓存",
                track_idx
            );
            match editor
                .editor_state
                .data
                .track_notes
                .get(&(track_idx as usize))
            {
                Some(n) => n,
                None => {
                    tracing::debug!(
                        "[onion-dirty] get_track_notes_for_hires: track={} 缓存未命中，返回空",
                        track_idx
                    );
                    return Vec::new();
                }
            }
        };
        let result: Vec<_> = notes
            .iter()
            .map(|n| {
                lumino_gfx::OnionSkinNote::from_ms(
                    n.tick,
                    n.tick + n.length,
                    n.key as u8,
                    super::onion_track_color(track_idx as usize),
                )
            })
            .collect();
        tracing::debug!(
            "[onion-dirty] get_track_notes_for_hires: track={}, count={}",
            track_idx,
            result.len()
        );
        result
    }

    /// 取出洋葱皮生成进度（runner 每帧调用并转发到进度窗口）
    pub fn drain_onion_progress(&self) -> Vec<(String, f32)> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.drain_onion_progress())
            .unwrap_or_default()
    }
}
