//! Runner 文件菜单：加载与导入
//!
//! 支持常规 MIDI/LMPJ 文件和压缩包文件的自动解压加载。
//! 压缩包中：
//! - 0 个 MIDI 文件 → 弹出错误提示
//! - 1 个 MIDI 文件 → 自动解压并加载
//! - 多个 MIDI 文件 → 弹出选择对话框（当前自动选择第一个，待完善）

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumino_core::{tr, trf};
use lumino_midi_loader::loader::archive_loading::{
    ArchiveLoadResult, ExtractedArchiveEntry, extract_entry_with_tempdir, scan_file_for_midi,
};

use crate::runner::{RunnerInner, async_helper::run_async_task};

/// 判断路径是否为 LMPJ 工程文件
fn is_lmpj_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lmpj"))
}

/// 工程加载的进度/错误文案（由调用点用 `tr!` 翻译，i18n key 保持在调用处）。
struct ProjectLoadText {
    /// 日志里的对象称呼（"素材文件" / "LMPJ 工程"）
    start: &'static str,
    /// 开始加载的进度文案
    loading: &'static str,
    /// 成功进度文案
    success: &'static str,
    /// 加载失败模板（含 `{e}`）
    failed: &'static str,
    /// 任务失败模板（含 `{e}`）
    task_failed: &'static str,
}

/// 加载失败的公共落定：渲染模板（`{e}`）、上报进度、清装载标记、发错误事件。
fn finish_project_load_error(
    progress_cb: &lumino_midi_loader::loader::ProgressCallback,
    template: &str,
    error: impl std::fmt::Display,
) {
    let msg = lumino_core::i18n::render(template, &[("e", error.to_string())]);
    progress_cb(&msg, 1.0);
    tracing::error!("{}", msg);
    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
        lumino_ui::event::menu::file::Event::MidiParseError(msg),
    ));
}

impl RunnerInner {
    /// 打开文件（支持 MIDI 文件和压缩包自动解压）
    pub(super) fn handle_open_file(&mut self) {
        self.file_state
            .file_handler
            .pick_file_async(lumino_ui::event::menu::file::Event::OpenFileSelected);
    }

    /// 打开工程文件夹（第一屏按钮：文件夹对话框 → 按工程目录加载）。
    pub(super) fn handle_open_project_folder(&mut self) {
        self.file_state
            .file_handler
            .pick_folder_async(lumino_ui::event::menu::file::Event::OpenFileSelected);
    }

    pub(super) fn handle_selected_file(&mut self, path: PathBuf) {
        // 解锁调色板，新 MIDI 加载后 MidiParsed 会重新锁定
        lumino_extras::palette::unlock_palette();

        tracing::info!("用户选择了文件：{:?}", path);
        // 文件夹 = 文件夹形式的 Lumino 工程（`load_project` 自己识别入口文件）。
        // 必须在这里分流：`scan_file_for_midi` 只认文件，目录会被当成常规 MIDI
        // 交给 `load_parsed_midi`，最终以「不是 MIDI」失败。
        if path.is_dir() {
            tracing::info!("按工程文件夹加载：{:?}", path);
            self.load_lmpj_project(path, None);
            return;
        }
        self.handle_midi_or_archive(path);
    }

    /// 处理 MIDI 文件或压缩包文件的加载
    fn handle_midi_or_archive(&self, path: PathBuf) {
        match scan_file_for_midi(&path) {
            ArchiveLoadResult::NotArchive => {
                // 常规 MIDI / LMPJ 文件，直接加载
                tracing::info!("常规 MIDI 文件，直接加载：{:?}", path);
                self.load_midi_file(path);
            }
            ArchiveLoadResult::NoMidiFiles => {
                // 压缩包但内部没有 MIDI 文件
                tracing::warn!("压缩包中未找到 MIDI 文件：{:?}", path);
                self.show_error_dialog(tr!(
                    "本文件不支持加载，请检查文件格式！\n\n支持的格式：\n- MIDI 文件 (.mid / .midi)\n- Lumino 项目文件 (.lmpj)\n- 包含上述文件的压缩包 (.zip / .rar / .7z 等)"
                ));
            }
            ArchiveLoadResult::SingleMidiFile(extracted) => {
                // 压缩包中只有一个 MIDI 文件，自动解压并加载
                let extracted_path = extracted.path().to_path_buf();
                tracing::info!(
                    "压缩包自动解压成功，加载 MIDI：{:?}（来自：{:?}）",
                    extracted_path,
                    path
                );
                self.load_midi_file_guarded(extracted_path, Some(extracted));
            }
            ArchiveLoadResult::MultipleMidiFiles(midi_list) => {
                // 多个 MIDI 文件，显示列表供用户选择
                tracing::info!(
                    "压缩包中发现多个 MIDI 文件（{} 个），提示用户选择",
                    midi_list.len()
                );
                self.handle_multiple_midi_in_archive(&path, midi_list);
            }
        }
    }

    /// 处理压缩包中有多个 MIDI 文件的情况
    ///
    /// TODO: 当前实现弹窗列出文件并自动选择第一个加载。
    /// 未来应实现一个真正的选择对话框，让用户从中选择要加载的文件。
    fn handle_multiple_midi_in_archive(&self, archive_path: &PathBuf, midi_list: Vec<String>) {
        // 构建文件列表字符串
        let file_list: String = midi_list
            .iter()
            .enumerate()
            .map(|(i, name)| format!("{}. {}", i + 1, name))
            .collect::<Vec<_>>()
            .join("\n");

        // 自动选择第一个 MIDI 文件加载
        if let Some(first_midi) = midi_list.first() {
            match extract_entry_with_tempdir(archive_path, first_midi) {
                Ok(extracted) => {
                    let extracted_path = extracted.path().to_path_buf();
                    tracing::info!(
                        "自动选择并加载第一个 MIDI 文件：{}（来自：{:?}）",
                        first_midi,
                        archive_path
                    );
                    self.show_info_dialog(&trf!(
                        "该压缩包中包含多个 MIDI 文件：\n\n{file_list}\n\n已自动加载第一个文件：{first_midi}\n如需加载其他文件，请直接打开对应的 MIDI 文件。",
                        file_list = file_list,
                        first_midi = first_midi
                    ));
                    self.load_midi_file_guarded(extracted_path, Some(extracted));
                }
                Err(e) => {
                    tracing::error!("提取 MIDI 文件失败：{e}");
                    self.show_error_dialog(&trf!(
                        "解压 MIDI 文件失败：{e}\n\n请尝试手动解压后加载。",
                        e = e
                    ));
                }
            }
        }
    }

    /// 导入文件（支持 MIDI 文件和压缩包自动解压）
    pub(super) fn handle_import_files(&mut self) {
        self.file_state
            .file_handler
            .pick_file_async(lumino_ui::event::menu::file::Event::ImportFilesSelected);
    }

    /// 加载 MIDI 文件（后台异步加载）
    pub(crate) fn load_midi_file(&self, path: PathBuf) {
        self.load_midi_file_guarded(path, None);
    }

    fn load_midi_file_guarded(&self, path: PathBuf, archive_guard: Option<ExtractedArchiveEntry>) {
        if is_lmpj_path(&path) {
            self.load_lmpj_project(path, archive_guard);
            return;
        }
        if lumino_export::is_material_path(&path) {
            self.load_material_project(path, archive_guard);
            return;
        }

        tracing::info!("开始后台加载 MIDI 文件：{:?}", path);
        // 看门狗在加载 MIDI 前确保已启动，并标记加载状态：看门狗只在
        // 加载 MIDI 期间监控内存，内存超限直接终止进程；非加载场景一律不监控。
        lumino_diagnostics::memory_monitor::watchdog::spawn_watchdog();
        lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(true);

        let progress_cb = Arc::clone(&self.window_state.progress_cb);
        tokio::spawn(async move {
            let _archive_guard = archive_guard;
            run_async_task(
                lumino_midi_loader::loader::load_parsed_midi(path, Some(&progress_cb)),
                |parsed| {
                    // 加载结束（成功）清除加载状态，看门狗恢复休眠
                    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parsed(std::sync::Arc::new(
                            parsed,
                        )),
                    )
                },
                |e| {
                    // 加载结束（失败）清除加载状态，看门狗恢复休眠
                    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parse_error(e),
                    )
                },
            )
            .await;
        });
    }

    /// 后台加载工程文件的公共骨架（素材 / LMPJ 共用）。
    ///
    /// 固定流程：看门狗启动 + 装载标记 → 0.3 进度 → 阻塞线程池解析 →
    /// 成功发 `MidiParsed` 并回调附加事件 / 失败发 `MidiParseError`。
    /// 差异由 `text`（文案）与两个闭包表达：`load` 产出解析结果与附加产物，
    /// `on_success` 在 `MidiParsed` 之后补发自身事件（形状/洋葱皮）。
    fn spawn_project_load<T, F, G>(
        &self,
        path: PathBuf,
        archive_guard: Option<ExtractedArchiveEntry>,
        text: ProjectLoadText,
        load: F,
        on_success: G,
    ) where
        T: Send + 'static,
        F: FnOnce(
                PathBuf,
            )
                -> Result<(lumino_midi_loader::ParsedMidi, T), lumino_export::ExportError>
            + Send
            + 'static,
        G: FnOnce(T) + Send + 'static,
    {
        tracing::info!("开始后台加载{}：{:?}", text.start, path);
        // 看门狗在加载前确保已启动，并标记加载状态（与 load_midi_file 一致）
        lumino_diagnostics::memory_monitor::watchdog::spawn_watchdog();
        lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(true);
        let progress_cb = Arc::clone(&self.window_state.progress_cb);
        tokio::spawn(async move {
            let _archive_guard = archive_guard;
            progress_cb(text.loading, 0.3);
            let path_for_blocking = path.clone();
            let load_result = tokio::task::spawn_blocking(move || load(path_for_blocking)).await;

            match load_result {
                Ok(Ok((parsed, extras))) => {
                    progress_cb(text.success, 1.0);
                    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParsed(Arc::new(parsed)),
                    ));
                    on_success(extras);
                }
                Ok(Err(e)) => finish_project_load_error(&progress_cb, text.failed, e),
                Err(e) => finish_project_load_error(&progress_cb, text.task_failed, e),
            }
        });
    }

    /// 加载素材文件（.lmmaterial，后台异步加载）
    ///
    /// 素材内部使用 lmpj 归档形式存储，加载后通过 metadata 的
    /// `[material]` 段分辨素材文件与标准工程文件（非素材则拒绝加载）。
    fn load_material_project(&self, path: PathBuf, archive_guard: Option<ExtractedArchiveEntry>) {
        self.spawn_project_load(
            path,
            archive_guard,
            ProjectLoadText {
                start: "素材文件",
                loading: tr!("正在加载素材"),
                success: tr!("素材加载成功"),
                failed: tr!("加载素材失败: {e}"),
                task_failed: tr!("加载素材任务失败: {e}"),
            },
            |path_for_blocking| {
                let project = lumino_export::load_project(&path_for_blocking)?;
                // 从 metadata 分辨素材/工程：非素材文件拒绝按素材加载
                if !project.metadata.is_material_file() {
                    return Err(lumino_export::ExportError::FileFormat(trf!(
                        "{path} 不是素材文件（.lmmaterial）",
                        path = path_for_blocking.display()
                    )));
                }
                let parsed = lumino_export::project_to_parsed_midi(&project, path_for_blocking)?;
                Ok((parsed, ()))
            },
            |()| {},
        );
    }

    /// 加载 LMPJ 工程文件（新格式或上一代兼容）
    fn load_lmpj_project(&self, path: PathBuf, archive_guard: Option<ExtractedArchiveEntry>) {
        self.spawn_project_load(
            path,
            archive_guard,
            ProjectLoadText {
                start: "LMPJ 工程",
                loading: tr!("正在加载 LMPJ 工程"),
                success: tr!("工程加载成功"),
                failed: tr!("加载 LMPJ 工程失败: {e}"),
                task_failed: tr!("加载 LMPJ 工程任务失败: {e}"),
            },
            |path_for_blocking| {
                let project = lumino_export::load_project(&path_for_blocking)?;
                // S0-5：形状对象随工程持久化，需在文档装载后灌回编辑器
                let shapes = project.shapes.clone();
                // 工程范围记忆：洋葱皮显示范围（缺字段 = 老工程 → 空串 = 默认全部）
                let onion_skin_mode = project
                    .metadata
                    .settings
                    .as_ref()
                    .and_then(|s| s.onion_skin_mode.clone())
                    .unwrap_or_default();
                let parsed = lumino_export::project_to_parsed_midi(&project, path_for_blocking)?;
                Ok::<_, lumino_export::ExportError>((parsed, (shapes, onion_skin_mode)))
            },
            |(shapes, onion_skin_mode)| {
                // 形状必须在 MidiParsed 之后发：文档装载路径会清空形状存储，
                // 顺序反了会把刚加载的形状再清掉。
                lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                    lumino_ui::event::menu::file::Event::ProjectShapes(shapes),
                ));
                // 洋葱皮显示范围同理放在最后：它只改工具栏状态，与文档装载无耦合，
                // 但排在后面对调试顺序更直观（文档 → 形状 → 视图偏好）。
                lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                    lumino_ui::event::menu::file::Event::ProjectOnionSkinMode(onion_skin_mode),
                ));
            },
        );
    }

    /// 将 MIDI 数据导入到编辑器
    ///
    /// 2026-08 单一权威源改造：`parsed` 以所有权传入，`MidiDocument` 在
    /// midi_handler 内零拷贝拆出（Arc::try_unwrap）并移入 UI 的 EditorData.document。
    pub(super) fn import_midi_to_editor(&mut self, parsed: lumino_midi_loader::ParsedMidi) {
        // 先取端口布局（handler 会 move 走 document）
        let midi_max_port = parsed.document.as_ref().map_or(0, |doc| doc.max_port());

        // 加载后诊断：EOT 之后仍有事件的轨 → 导入 toast（不阻断，可关闭/自动消失）
        let after_eot = parsed.timing_report.tracks_with_events_after_eot();
        let timing_report = parsed.timing_report.clone();
        self.midi_state.timing_report = timing_report;

        // 导入文档（`MidiHandler::import_midi_to_editor` 开头先
        // `reset_playback_manager` 停播，再设置文档）
        {
            let ui = self.window_state.window.ui_mut();
            self.midi_state
                .midi_handler
                .import_midi_to_editor(ui, parsed);

            if after_eot > 0 {
                ui.root_mut().push_toast(
                    lumino_ui::toast::ToastLevel::Warning,
                    trf!(
                        "导入完成：{n} 条轨在 End-of-Track 之后仍有事件（兼容模式已保留数据，导出前可裁剪）",
                        n = after_eot
                    ),
                );
            }
        }

        // REND-002：停播后再按文档端口布局强制重建合成管线（清上一文档通道模态），
        // 最后创建播放输出连接，保证连接指向正确的通道空间。
        self.midi_state.midi.apply_midi_port_layout(midi_max_port);

        // MIDI 导入后，为播放管理器绑定一个独立的 MIDI 输出连接。
        // 返工 #149：软件合成后端初始化在途时创建被有意延迟（零 WinMM 触碰），
        // 不算失败——就绪后由既有重连路径自动接线。
        if let Some(output) = self.midi_state.midi.create_additional_output() {
            self.window_state
                .window
                .ui_mut()
                .set_playback_midi_output(output);
            tracing::info!("Playback MIDI output connected");
        } else if self.midi_state.midi.software_backend_pending() {
            tracing::info!("MIDI: 软件合成后端初始化中，播放输出将在就绪后自动接线");
        } else {
            tracing::warn!("Failed to create playback MIDI output connection");
        }
    }
}
