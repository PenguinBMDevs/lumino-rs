//! 配置持久化管理
//!
//! RunnerInner 中与配置保存、差异检测等相关的实现。

use super::super::window_manager::WindowManager;
use super::*;

struct ConfigDiff {
    synth_changed: bool,
    xsynth_changed: bool,
    audio_engine_changed: bool,
    titlebar_changed: bool,
    font_changed: bool,
    lgs_changed: bool,
    winmm_changed: bool,
    audio_output_changed: bool,
    /// 界面语言偏好或生效语言变更（UI-018：需落盘 + 重建 macOS 原生菜单）
    language_changed: bool,
}

fn display_or_empty(s: &str) -> &str {
    if s.is_empty() { "(空)" } else { s }
}

/// UI-020：语言字段是否应落盘。
///
/// 环境变量（`LUMINO_LANG`）只影响本次启动：来源为 env 且用户本次未显式
/// 改语言时不落盘（防止排障覆盖被任意一次设置保存物化成固定语言）；
/// 用户真实改过语言（`language_changed`）时照常写入意图。
fn should_persist_language(language_from_env: bool, language_changed: bool) -> bool {
    !language_from_env || language_changed
}

/// 解析自动保存间隔输入（分钟）：越界值收敛到 1..=120，非法输入返回
/// `None`（落盘时保留旧值，不阻塞其他设置项的保存）。
fn parse_autosave_interval_mins(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().map(|v| v.clamp(1, 120))
}

/// 洋葱皮渲染后端是否应落盘（「加载 MIDI 后禁止切换后端」的第三道闸）。
///
/// 两个条件**任一**为真即不写，保留旧配置：
/// - `backend_locked`：设置面板处于锁定会话态（主窗口注入）；
/// - `midi_loaded`：主窗口会话已加载**真实** MIDI/工程（空白工程不算）。
///
/// 为什么两个都要：设置对话框可能在「无 MIDI」时打开、之后才加载 MIDI，
/// 此时对话框面板里的 `backend_locked` 仍是打开那一刻的 `false`——
/// 只信面板字段会把「加载 MIDI 后本不该生效的后端」写到下次启动。
///
/// 为什么判据是 `midi_loaded` 而不是「有没有文档」：启动/新建/关闭工程都会挂一份
/// 2 轨空白 `MidiDocument`，用「有文档」判会把空白工程也挡在落盘之外——
/// 用户在空白工程里选的后端永远存不下来（见 `host::session_lock`）。
fn should_persist_onion_backend(backend_locked: bool, midi_loaded: bool) -> bool {
    !backend_locked && !midi_loaded
}

impl RunnerInner {
    fn config_diff(
        new: &lumino_ui::settings::SettingsPanel,
        old: &lumino_core::storage::config::UiConfig,
        current_theme: &str,
        persist_onion_backend: bool,
    ) -> Option<ConfigDiff> {
        let theme_changed = current_theme != old.theme;
        let synth_changed = new.synth.backend != old.preferred_backend
            || new.synth.soundfont_path != old.soundfont_path;
        let xsynth_changed = new.synth.xsynth_buffer_ms != old.xsynth_buffer_ms
            || new.synth.xsynth_sample_rate != old.xsynth_sample_rate
            || new.synth.xsynth_threads != old.xsynth_threads
            || new.synth.xsynth_max_voices_per_key != old.xsynth_max_voices_per_key
            || new.synth.xsynth_global_voice_limit != old.xsynth_global_voice_limit
            || (new.synth.xsynth_voice_target_ratio - old.xsynth_voice_target_ratio).abs() > 1e-9
            || new.synth.xsynth_soft_nps_gate != old.xsynth_soft_nps_gate;
        let audio_engine_changed = new.synth.audio_engine != old.audio_engine;
        let titlebar_changed = new.synth.use_native_titlebar != old.use_native_titlebar;
        let font_changed = new.editing.program_font_name != old.program_font_name
            || new.editing.program_font_path != old.program_font_path;
        // LGS (GPU) 专属参数变化需要重建 GPU 合成器后端：缓冲区大小、每键最大同音数，
        // 或（仅当当前/此前为 LGS 时）专属响度过滤阈值变化（与 XSynth 全局力度过滤相互独立）。
        let lgs_changed = new.synth.lgs_block_size != old.lgs_block_size
            || new.synth.lgs_max_voices_per_key != old.lgs_max_voices_per_key
            || new.synth.lgs_global_voice_limit != old.lgs_global_voice_limit
            || new.synth.lgs_soft_nps_gate != old.lgs_soft_nps_gate
            || (new.synth.lgs_velocity_filter_threshold != old.lgs_velocity_filter_threshold
                && (new.synth.backend == lumino_core::storage::config::SynthBackend::Lgs
                    || old.preferred_backend == lumino_core::storage::config::SynthBackend::Lgs));
        // 系统 MIDI (WinMM) 播表（输出设备）变化需要重建 System 输出连接
        let winmm_changed = new.synth.backend == lumino_core::storage::config::SynthBackend::System
            && new.midi.selected_winmm_output != old.system_output_device_id;
        // 音频播放输出设备（CPAL 音频设备）变化需要重建软件合成器（XSynth / LGS）的音频流
        let audio_output_changed =
            new.synth.selected_audio_output_device != old.audio_output_device;
        // UI-018：偏好（跟随系统 / 固定语言）与生效语言任一变化，都要落盘并重建
        // macOS 原生菜单。老配置的 `None` 偏好按 `Fixed(language)` 归一化后再比较，
        // 否则「从老配置升上来」会被误判成偏好变更。
        let language_changed = new.display.language_preference
            != old.effective_language_preference()
            || new.display.language != old.language;
        // 洋葱皮后端：只有「允许落盘」时后端差异才算变更。
        // 否则锁定态下（面板值 vs 旧配置不同、但运行时保持不变）会每帧都被判成
        // 「有变更」→ 每帧 `patch` + 写盘，白白打爆磁盘。
        let onion_backend_changed =
            persist_onion_backend && new.hires.backend != old.onion_skin_backend;
        let other_changed = language_changed
            || onion_backend_changed
            || new.editing.selection_box_mode != old.selection_box_mode
            || new.midi.velocity_filter_threshold != old.velocity_filter_threshold
            || new.editing.eraser_behavior != old.eraser_behavior
            || new.auto_scroll.fixed_position != old.auto_scroll.fixed_indicator_position
            || new.auto_scroll.page_trigger_offset != old.auto_scroll.page_trigger_offset
            || new.auto_scroll.page_return_position != old.auto_scroll.page_return_position
            || new.display.icon_hidpi != old.icon_hidpi
            || new.display.enable_256key != old.enable_256key
            || new.display.note_count_display != old.note_count_display
            || new.display.velocity_curve_style != old.velocity_curve_style
            || new.display.playback_key_colors_enabled != old.playback_key_colors_enabled
            || new.editing.track_add_behavior != old.track_add_behavior
            || new.editing.history_total_limit != old.history_total_limit
            || new.editing.history_entry_limit != old.history_entry_limit
            || new.editing.merge_window_ms != old.merge_window_ms
            || new.editing.intercept_notification_enabled != old.intercept_notification_enabled
            || new.editing.automation_line_thickness != old.automation_line_thickness
            || new.editing.tempo_max_bpm != old.tempo_max_bpm
            || new.logging.monitor_refresh_interval_ms != old.monitor_refresh_interval_ms
            || new.synth.gpu_buffer_status_enabled != old.gpu_buffer_status_enabled
            || new.synth.gpu_governor_metrics_enabled != old.gpu_governor_metrics_enabled
            || new.synth.gpu_load_display != old.gpu_load_display
            || new.compat.check_on_startup != old.gpu_check_on_startup
            || new.compat.domino_clipboard_enabled != old.domino_clipboard_enabled
            || new.compat.warning_suppressed != old.gpu_warning_suppressed.unwrap_or(false)
            || new.editing.autosave_enabled != old.autosave_enabled
            || parse_autosave_interval_mins(&new.editing.autosave_interval_mins)
                .is_some_and(|v| v != old.autosave_interval_mins);
        if theme_changed
            || synth_changed
            || xsynth_changed
            || audio_engine_changed
            || titlebar_changed
            || font_changed
            || lgs_changed
            || winmm_changed
            || audio_output_changed
            || other_changed
        {
            Some(ConfigDiff {
                synth_changed,
                xsynth_changed,
                audio_engine_changed,
                titlebar_changed,
                font_changed,
                lgs_changed,
                winmm_changed,
                audio_output_changed,
                language_changed,
            })
        } else {
            None
        }
    }

    fn auto_scroll_mode_changed(
        window: &WindowManager,
        old_mode: lumino_core::storage::config::AutoScrollMode,
    ) -> bool {
        window.ui().root().editor.editor_state.auto_scroll.mode != old_mode
    }

    pub(crate) fn save_storage(&mut self) {
        let new = self.window_state.window.ui().settings();
        // UI-020：本次启动语言是否来自环境变量（LUMINO_LANG）。
        // 排障覆盖只影响本次启动，不得被任意一次设置保存物化为「固定语言」。
        let language_from_env = self.window_state.storage.config.language_from_env();
        let old = &self.window_state.storage.config.get().ui;
        let current_theme = self.window_state.window.ui().root().theme().to_string();
        let auto_scroll_mode_changed =
            Self::auto_scroll_mode_changed(&self.window_state.window, old.auto_scroll.mode);
        // 「加载 MIDI 后禁止切换后端」的持久化兜底：主窗口会话是否加载过真实 MIDI
        // 才是硬事实（`Host::onion_backend_locked()` 的同一真源），面板字段
        // （`hires.backend_locked`）在「先开设置、后载 MIDI」的时序下会过期。
        // 刻意不查 `document.is_some()`——空白工程也挂着 2 轨空文档，那样会永远不落盘。
        let midi_loaded = self.window_state.window.ui().onion_backend_locked();
        let persist_onion_backend =
            should_persist_onion_backend(new.hires.backend_locked, midi_loaded);
        let diff = match Self::config_diff(new, old, &current_theme, persist_onion_backend) {
            Some(d) => d,
            None if !auto_scroll_mode_changed => return,
            None => ConfigDiff {
                synth_changed: false,
                xsynth_changed: false,
                audio_engine_changed: false,
                titlebar_changed: false,
                font_changed: false,
                lgs_changed: false,
                winmm_changed: false,
                audio_output_changed: false,
                language_changed: false,
            },
        };
        if diff.synth_changed {
            tracing::info!(
                "合成器设置已改变: backend {} -> {}, soundfont {} -> {}",
                old.preferred_backend,
                new.synth.backend,
                display_or_empty(&old.soundfont_path),
                display_or_empty(&new.synth.soundfont_path),
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.xsynth_changed {
            tracing::info!(
                "XSynth 参数已改变: buffer={:.1}ms-> {:.1}ms, sr={}-> {}, threads={}-> {}, voices={:?}-> {:?}, 全局上限={:?}-> {:?}, 软目标比例={:.3}-> {:.3}, 保险闸={}-> {}",
                old.xsynth_buffer_ms,
                new.synth.xsynth_buffer_ms,
                old.xsynth_sample_rate,
                new.synth.xsynth_sample_rate,
                old.xsynth_threads,
                new.synth.xsynth_threads,
                old.xsynth_max_voices_per_key,
                new.synth.xsynth_max_voices_per_key,
                old.xsynth_global_voice_limit,
                new.synth.xsynth_global_voice_limit,
                old.xsynth_voice_target_ratio,
                new.synth.xsynth_voice_target_ratio,
                old.xsynth_soft_nps_gate,
                new.synth.xsynth_soft_nps_gate,
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.lgs_changed {
            tracing::info!(
                "LGS (GPU) 参数已改变: block_size {} -> {}, max_voices {} -> {}, 全局上限 {:?} -> {:?}, 防爆闸 {} -> {}, velocity_filter {} -> {}",
                old.lgs_block_size,
                new.synth.lgs_block_size,
                old.lgs_max_voices_per_key,
                new.synth.lgs_max_voices_per_key,
                old.lgs_global_voice_limit,
                new.synth.lgs_global_voice_limit,
                old.lgs_soft_nps_gate,
                new.synth.lgs_soft_nps_gate,
                old.lgs_velocity_filter_threshold,
                new.synth.lgs_velocity_filter_threshold
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.winmm_changed {
            tracing::info!(
                "WinMM 播表(输出设备)已改变: {:?} -> {:?}",
                old.system_output_device_id,
                new.midi.selected_winmm_output
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.audio_output_changed {
            tracing::info!(
                "音频播放输出设备已改变: {:?} -> {:?}",
                old.audio_output_device,
                new.synth.selected_audio_output_device
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.audio_engine_changed {
            tracing::info!(
                "音频引擎已改变: {:?} -> {:?}",
                old.audio_engine,
                new.synth.audio_engine
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.titlebar_changed {
            tracing::info!(
                "标题栏设置已改变: native_titlebar {} -> {}",
                old.use_native_titlebar,
                new.synth.use_native_titlebar
            );
            self.window_state.needs_window_restart = true;
        }
        if diff.font_changed {
            tracing::info!(
                "字体设置已改变（重启程序后生效）: font_name {} -> {}, font_path {} -> {}",
                display_or_empty(&old.program_font_name),
                display_or_empty(&new.editing.program_font_name),
                display_or_empty(&old.program_font_path),
                display_or_empty(&new.editing.program_font_path),
            );
            // UI-001：字体在启动时经 lumino_ui_core::font::init 解析一次，重启程序后生效；
            // 不再重建窗口（重建成本高且主窗口会闪烁，iced 0.14 的 Renderer 默认字体不可热更），
            // 设置面板内有「字体修改将在重启程序后生效」的显式提示。
        }
        if diff.language_changed {
            tracing::info!(
                "界面语言已改变: {} -> {}（偏好 {:?}）",
                old.language,
                new.display.language,
                new.display.language_preference,
            );
            // macOS 原生菜单是 NSMenu（iced 不参与绘制），启动时静态构建、语言变更后
            // 必须显式重建，否则菜单栏会停在旧语言（UI-018 缺陷 D-1）。
            #[cfg(target_os = "macos")]
            if let Err(e) = crate::platform::macos::init(new.display.language) {
                tracing::error!("重建 macOS 原生菜单失败: {:?}", e);
            }
        }
        if current_theme != old.theme {
            tracing::info!("主题已改变: {} -> {}", old.theme, current_theme);
        }
        self.window_state.storage.config.patch(|config| {
            config.ui.theme.clone_from(&current_theme);
            // UI-018 + UI-020：语言字段落盘守卫。环境变量（LUMINO_LANG）只影响
            // 本次启动：来源为 env 且用户本次未显式改语言时，`language` 与
            // `language_preference` 都不写——`language` 必须一并守卫：老配置
            // （preference=None）的解析会回退到该字段，单独放行仍会把 env 值
            // 带到下次启动。非 env 来源或用户显式改语言时，照常写入
            // 「跟随系统 / 固定语言」的**意图**（不写的话下次启动会退化成
            // 「老配置保留」，用户选的「跟随系统」形同虚设）。
            if should_persist_language(language_from_env, diff.language_changed) {
                config.ui.language = new.display.language;
                config.ui.language_preference = Some(new.display.language_preference);
            }
            config.ui.preferred_backend = new.synth.backend;
            config.ui.audio_engine = new.synth.audio_engine;
            config.ui.soundfont_path = new.synth.soundfont_path.clone();
            config.ui.use_native_titlebar = new.synth.use_native_titlebar;
            config.ui.program_font_name = new.editing.program_font_name.clone();
            config.ui.program_font_path = new.editing.program_font_path.clone();
            config.ui.selection_box_mode = new.editing.selection_box_mode;
            config.ui.xsynth_buffer_ms = new.synth.xsynth_buffer_ms;
            config.ui.xsynth_sample_rate = new.synth.xsynth_sample_rate;
            config.ui.xsynth_threads = new.synth.xsynth_threads;
            config.ui.xsynth_max_voices_per_key = new.synth.xsynth_max_voices_per_key;
            config.ui.xsynth_global_voice_limit = new.synth.xsynth_global_voice_limit;
            config.ui.xsynth_voice_target_ratio = new.synth.xsynth_voice_target_ratio;
            config.ui.xsynth_soft_nps_gate = new.synth.xsynth_soft_nps_gate;
            config.ui.lgs_block_size = new.synth.lgs_block_size;
            // REF-LGS-009：离线块长只影响导出，不触发实时管线重建（不进 lgs_changed）
            config.ui.lgs_offline_block_size = new.synth.lgs_offline_block_size;
            config.ui.lgs_max_voices_per_key = new.synth.lgs_max_voices_per_key;
            config.ui.lgs_global_voice_limit = new.synth.lgs_global_voice_limit;
            config.ui.lgs_soft_nps_gate = new.synth.lgs_soft_nps_gate;
            config.ui.lgs_velocity_filter_threshold = new.synth.lgs_velocity_filter_threshold;
            // GPU 指示器为纯显示项——只持久化，不进 lgs_changed（不触发后端重建）
            config.ui.gpu_buffer_status_enabled = new.synth.gpu_buffer_status_enabled;
            config.ui.gpu_governor_metrics_enabled = new.synth.gpu_governor_metrics_enabled;
            config.ui.gpu_load_display = new.synth.gpu_load_display;
            config.ui.velocity_filter_threshold = new.midi.velocity_filter_threshold;
            config.ui.eraser_behavior = new.editing.eraser_behavior;
            config.ui.auto_scroll.mode = self
                .window_state
                .window
                .ui()
                .root()
                .editor
                .editor_state
                .auto_scroll
                .mode;
            config.ui.auto_scroll.fixed_indicator_position = new.auto_scroll.fixed_position;
            config.ui.auto_scroll.page_trigger_offset = new.auto_scroll.page_trigger_offset;
            config.ui.auto_scroll.page_return_position = new.auto_scroll.page_return_position;
            config.ui.icon_hidpi = new.display.icon_hidpi;
            config.ui.enable_256key = new.display.enable_256key;
            config.ui.note_count_display = new.display.note_count_display;
            config.ui.velocity_curve_style = new.display.velocity_curve_style;
            config.ui.hires_onion_enabled = new.hires.onion_enabled;
            config.ui.hires_measures_per_group = new.hires.measures_per_group;
            config.ui.hires_tile_width_px = new.hires.tile_width_px;
            config.ui.hires_cooldown_secs = new.hires.cooldown_secs;
            config.ui.hires_gpu_mem_limit_mb = new.hires.gpu_mem_limit_mb;
            // 洋葱皮渲染后端：锁定/有 MIDI 文档时保留旧配置（见
            // `should_persist_onion_backend`——锁定后运行时值本来也切不动）。
            if persist_onion_backend {
                config.ui.onion_skin_backend = new.hires.backend;
            }
            config.ui.playback_key_colors_enabled = new.display.playback_key_colors_enabled;
            config.ui.track_add_behavior = new.editing.track_add_behavior;
            config.ui.selected_palette = new.display.selected_palette.clone();
            config.ui.history_total_limit = new.editing.history_total_limit;
            config.ui.history_entry_limit = new.editing.history_entry_limit;
            config.ui.merge_window_ms = new.editing.merge_window_ms;
            config.ui.intercept_notification_enabled = new.editing.intercept_notification_enabled;
            config.ui.automation_line_thickness = new.editing.automation_line_thickness;
            config.ui.tempo_max_bpm = new.editing.tempo_max_bpm;
            config.ui.monitor_refresh_interval_ms = new.logging.monitor_refresh_interval_ms;
            config.ui.log_retention_count = new.logging.log_retention_count;
            config.ui.system_output_device_id = new.midi.selected_winmm_output;
            config.ui.audio_output_device = new.synth.selected_audio_output_device.clone();
            config.ui.gpu_check_on_startup = new.compat.check_on_startup;
            config.ui.domino_clipboard_enabled = new.compat.domino_clipboard_enabled;
            config.ui.gpu_warning_suppressed = Some(new.compat.warning_suppressed);
            config.ui.autosave_enabled = new.editing.autosave_enabled;
            // 间隔输入框原文非法时保留旧值（面板侧不做实时校验，落盘时统一 parse）
            if let Some(v) = parse_autosave_interval_mins(&new.editing.autosave_interval_mins) {
                config.ui.autosave_interval_mins = v;
            }
        });
        lumino_extras::palette::set_current_palette_by_name(&new.display.selected_palette);
        if let Err(e) = self.window_state.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.window_state.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}

/// 落盘守卫测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "persist/tests.rs"]
mod tests;
