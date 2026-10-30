//! 文案目录分域：src/**（应用宿主、菜单、生命周期、服务）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        "无标题" => Some("Untitled"),
        "Lumino 工程入口" => Some("Lumino Project Entry"),
        "准备导出工程" => Some("Preparing to export project"),
        "正在导出工程" => Some("Exporting project"),
        "工程导出成功" => Some("Project exported successfully"),
        "导出失败: {e}" => Some("Export failed: {e}"),
        "导出任务失败: {e}" => Some("Export task failed: {e}"),
        "本文件不支持加载，请检查文件格式！\n\n支持的格式：\n- MIDI 文件 (.mid / .midi)\n- Lumino 项目文件 (.lmpj)\n- 包含上述文件的压缩包 (.zip / .rar / .7z 等)" => {
            Some(
                "This file cannot be loaded. Please check the file format!\n\nSupported formats:\n- MIDI files (.mid / .midi)\n- Lumino project files (.lmpj)\n- Archives containing the files above (.zip / .rar / .7z, etc.)",
            )
        }
        "该压缩包中包含多个 MIDI 文件：\n\n{file_list}\n\n已自动加载第一个文件：{first_midi}\n如需加载其他文件，请直接打开对应的 MIDI 文件。" => {
            Some(
                "This archive contains multiple MIDI files:\n\n{file_list}\n\nThe first file has been loaded automatically: {first_midi}\nTo load another file, open the corresponding MIDI file directly.",
            )
        }
        "解压 MIDI 文件失败：{e}\n\n请尝试手动解压后加载。" => Some(
            "Failed to extract the MIDI file: {e}\n\nPlease extract it manually and then load it.",
        ),
        "加载失败" => Some("Load Failed"),
        "提示" => Some("Notice"),
        "正在加载素材" => Some("Loading material"),
        "素材加载成功" => Some("Material loaded successfully"),
        "加载素材失败: {e}" => Some("Failed to load material: {e}"),
        "加载素材任务失败: {e}" => Some("Material load task failed: {e}"),
        "正在加载 LMPJ 工程" => Some("Loading LMPJ project"),
        "工程加载成功" => Some("Project loaded successfully"),
        "加载 LMPJ 工程失败: {e}" => Some("Failed to load LMPJ project: {e}"),
        "加载 LMPJ 工程任务失败: {e}" => Some("LMPJ project load task failed: {e}"),
        "{path} 不是素材文件（.lmmaterial）" => {
            Some("{path} is not a material file (.lmmaterial)")
        }
        "正在导出素材" => Some("Exporting material"),
        "素材导出成功" => Some("Material exported successfully"),
        "素材导出失败: {e}" => Some("Material export failed: {e}"),
        "素材导出任务失败: {e}" => Some("Material export task failed: {e}"),
        "正在保存 LMPJ 文件" => Some("Saving LMPJ file"),
        "工程保存成功" => Some("Project saved successfully"),
        "保存失败: {e}" => Some("Save failed: {e}"),
        "保存任务失败: {e}" => Some("Save task failed: {e}"),
        "已有导出任务进行中，请等待完成" => {
            Some("An export task is already running. Please wait for it to finish")
        }
        "正在初始化 {backend} 渲染..." => Some("Initializing {backend} rendering..."),
        "内存文档为空且 MIDI 文件不存在，请先加载 MIDI 或检查 MIDI 路径" => {
            Some(
                "The in-memory document is empty and the MIDI file does not exist. Load a MIDI file first or check the MIDI path",
            )
        }
        "正在准备协作工程 {name}…" => Some("Preparing collaborative project {name}…"),
        "正在上传协作工程 {name}（{pct}%）" => {
            Some("Uploading collaborative project {name} ({pct}%)")
        }
        "生成协作工程失败：{e}" => {
            Some("Failed to generate the collaborative project: {e}")
        }
        "无工程可上传或生成失败" => Some("No project to upload, or generation failed"),
        "正在上传协作工程 {name}…" => Some("Uploading collaborative project {name}…"),
        "协作工程上传完成" => Some("Collaborative project upload complete"),
        "协作工程上传失败：{e}" => Some("Failed to upload the collaborative project: {e}"),
        "正在同步协作工程 {invite_code}…" => {
            Some("Syncing collaborative project {invite_code}…")
        }
        "本地工程已是最新" => Some("Local project is up to date"),
        "正在下载协作工程 {code}…" => Some("Downloading collaborative project {code}…"),
        "协作工程已加载" => Some("Collaborative project loaded"),
        "解析下载工程失败：{e}" => Some("Failed to parse the downloaded project: {e}"),
        "加载工程任务失败：{e}" => Some("Project load task failed: {e}"),
        "下载房间工程失败：{e}" => Some("Failed to download the room project: {e}"),
        "渲染线程未启动" => Some("Render thread not started"),
        "导出失败：流式读取模式已暂时禁用，请先加载工程" => {
            Some("Export failed: streaming read mode is temporarily disabled. Load a project first")
        }
        "导出失败：无 MIDI 数据" => Some("Export failed: no MIDI data"),
        "导出线程启动失败: {e}" => Some("Failed to start the export thread: {e}"),
        "导出失败：渲染线程通信错误" => {
            Some("Export failed: render thread communication error")
        }
        "导出已取消" => Some("Export cancelled"),
        "导出失败：Waterfall 模式不应进入 CPU 渲染分支（内部错误）" => Some(
            "Export failed: Waterfall mode must not enter the CPU rendering branch (internal error)",
        ),
        "导出失败：计数器模式缺少渲染配置（内部错误）" => {
            Some("Export failed: counter mode is missing its render configuration (internal error)")
        }
        "导出失败：计数器模式缺少统计状态（内部错误）" => {
            Some("Export failed: counter mode is missing its statistics state (internal error)")
        }
        "导出失败：计数器模式缺少字体渲染器（内部错误）" => {
            Some("Export failed: counter mode is missing its font renderer (internal error)")
        }
        "导出失败：NoteRectangle 模式不应进入 CPU 渲染分支（内部错误）" => {
            Some(
                "Export failed: NoteRectangle mode must not enter the CPU rendering branch (internal error)",
            )
        }
        "导出失败：MIDITrail 模式不应进入 CPU 渲染分支（内部错误）" => Some(
            "Export failed: MIDITrail mode must not enter the CPU rendering branch (internal error)",
        ),
        "导出失败：数据曲线模式缺少渲染配置（内部错误）" => Some(
            "Export failed: data curve mode is missing its render configuration (internal error)",
        ),
        "导出失败：数据曲线模式缺少统计状态（内部错误）" => {
            Some("Export failed: data curve mode is missing its statistics state (internal error)")
        }
        "导出失败：数据曲线模式缺少渲染器（内部错误）" => {
            Some("Export failed: data curve mode is missing its renderer (internal error)")
        }
        "导出失败：MidiConsole 模式缺少渲染器（内部错误）" => {
            Some("Export failed: MidiConsole mode is missing its renderer (internal error)")
        }
        "导出失败：MidiConsole 模式缺少渲染配置（内部错误）" => Some(
            "Export failed: MidiConsole mode is missing its render configuration (internal error)",
        ),
        "导出失败：帧通道通信错误" => {
            Some("Export failed: frame channel communication error")
        }
        "导出失败：当前渲染模式不应进入此分支（内部错误）" => Some(
            "Export failed: the current render mode must not enter this branch (internal error)",
        ),
        "导出失败：帧数据通道关闭" => Some("Export failed: frame data channel closed"),
        "正在扫描 MIDI 头部信息..." => Some("Scanning MIDI header..."),
        "读取 MIDI 文件失败: {e}" => Some("Failed to read the MIDI file: {e}"),
        "MmapSmf 解析失败: {e}" => Some("Failed to parse MmapSmf: {e}"),
        "正在并行解析所有轨道..." => Some("Parsing all tracks in parallel..."),
        "正在合并轨道数据..." => Some("Merging track data..."),
        "创建音符缓存文件失败: {e}" => Some("Failed to create the note cache file: {e}"),
        "写入缓存 header 失败: {e}" => Some("Failed to write the cache header: {e}"),
        "写入音符记录失败: {e}" => Some("Failed to write note records: {e}"),
        "正在构建帧索引..." => Some("Building frame index..."),
        "MIDI 缓存准备完成" => Some("MIDI cache ready"),
        "打开音符缓存文件失败: {e}" => Some("Failed to open the note cache file: {e}"),
        "帧索引越界" => Some("Frame index out of range"),
        "seek 音符缓存失败: {e}" => Some("Failed to seek in the note cache: {e}"),
        "读取音符缓存失败: {e}" => Some("Failed to read the note cache: {e}"),
        // ── 导出：GPU 设备丢失后的恢复提示（audio_export / video_export_handler） ──
        "GPU 正在恢复，请稍后重试导出" => {
            Some("GPU is recovering; please retry the export shortly")
        }
        "导出失败：GPU 设备丢失（驱动超时恢复）——导出已中断，渲染已自动恢复，可直接重试" => {
            Some(
                "Export failed: GPU device lost (driver timeout recovery). The export was interrupted, but rendering has recovered automatically — you can retry directly",
            )
        }
        // ── 自动保存恢复 / 崩溃日志（RUNNER）──
        "恢复自动保存" => Some("Restore autosave"),
        "不恢复" => Some("Don't restore"),
        "检测到未保存的自动保存" => Some("Unsaved autosave detected"),
        "发现上次会话的自动保存（修改于 {time}）。\n\n恢复不会改动任何已有文件。" => {
            Some(
                "Found an autosave from the previous session (modified at {time}).\n\nRestoring it will not modify any existing files.",
            )
        }
        "按「确定」恢复，按「取消」不恢复。" => {
            Some("Press OK to restore, or Cancel to discard.")
        }
        "已恢复自动保存，请另存为" => {
            Some("Autosave restored. Please use Save As to keep it.")
        }
        "主自动保存损坏" => Some("Main autosave is corrupted"),
        "主自动保存无法打开，已改名保留。{hint}\n\n将尝试从备份恢复。" => {
            Some(
                "The main autosave could not be opened and was renamed to preserve it.{hint}\n\nTrying to restore from the backup.",
            )
        }
        "自动保存无法恢复" => Some("Autosave could not be restored"),
        "自动保存无法恢复，文件已保留：{path}。请从原工程或备份手动恢复。" => {
            Some(
                "Autosave could not be restored; the file was preserved: {path}. Please recover manually from the original project or a backup.",
            )
        }
        "自动保存文件无法打开，已改名保留：\n\n{path}\n\n请从原工程或备份手动恢复。" => {
            Some(
                "The autosave file could not be opened and was renamed to preserve it:\n\n{path}\n\nPlease recover manually from the original project or a backup.",
            )
        }
        "上次运行发生崩溃" => Some("A crash occurred during the last run"),
        "发现 {count} 份崩溃日志，已归档到：\n\n{path}\n\n可打开日志目录查看详情。" => {
            Some(
                "Found {count} crash log(s), archived to:\n\n{path}\n\nYou can open the log folder for details.",
            )
        }
        "打开日志目录" => Some("Open log folder"),
        "打开目录" => Some("Open folder"),
        "关闭窗口" => Some("Close window"),
        "未知时间" => Some("Unknown time"),
        "自动保存失败，请检查日志" => Some("Autosave failed; please check the log"),
        _ => None,
    }
}
