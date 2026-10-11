//! 文案目录分域：crates/core/{core,project,message,diagnostics}/**（工程读写与核心类型）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // ── crates/core/diagnostics/src/memtrace.rs（内存监控对话框「按子系统分类」）──
        "MIDI 数据" => Some("MIDI data"),
        "音色库采样" => Some("SoundFont samples"),
        "音频引擎" => Some("Audio engine"),
        "GPU 显存/缓冲" => Some("GPU VRAM / buffers"),
        "UI / 状态" => Some("UI / state"),
        "其他（未显式归类）" => Some("Other (uncategorized)"),
        // ── crates/core/project/src/project/archive.rs（LMPJ 归档读取失败提示）──
        "{what}: 偏移 {offset} 超出地址空间" => {
            Some("{what}: offset {offset} is outside the address space")
        }
        "{what}: 长度 {len} 超出地址空间" => {
            Some("{what}: length {len} is outside the address space")
        }
        "{what}: 偏移 {offset}+{len} 溢出" => Some("{what}: offset {offset}+{len} overflowed"),
        "{what}: 数据区越界（{start}..{end}，文件共 {total} 字节），文件可能已损坏" => {
            Some(
                "{what}: data range out of bounds ({start}..{end}, file is {total} bytes); the file may be corrupted",
            )
        }
        "{what} 解压失败: {e}" => Some("Failed to decompress {what}: {e}"),
        "{what} 解压后超过上限（{MAX_DECOMPRESSED_PART_BYTES} 字节），文件可能已损坏" => {
            Some(
                "{what} exceeds the decompression limit ({MAX_DECOMPRESSED_PART_BYTES} bytes); the file may be corrupted",
            )
        }
        "{what} 大小与文件头不一致（期望 {expected_original_size}，实际 {actual}），文件可能已损坏" => {
            Some(
                "{what} size does not match the header (expected {expected_original_size}, got {actual}); the file may be corrupted",
            )
        }
        "file entry: incomplete（需要 {needed} 字节，实际 {actual}）" => {
            Some("file entry: incomplete (needs {needed} bytes, got {actual})")
        }
        "file table: 条目区越界（pos={pos}）" => {
            Some("file table: entry area out of bounds (pos={pos})")
        }
        "文件表" => Some("File table"),
        "文件 {file_path}" => Some("File {file_path}"),
        "这不是有效的 Lumino 工程文件（魔数不匹配）" => {
            Some("Not a valid Lumino project file (magic number mismatch)")
        }
        "不支持的工程文件版本 v{version}（当前支持 v{ARCHIVE_VERSION}）" => {
            Some("Unsupported project file version v{version} (v{ARCHIVE_VERSION} is supported)")
        }
        "不支持的归档压缩标志 0x{flags}（仅支持 zstd）" => {
            Some("Unsupported archive compression flag 0x{flags} (only zstd is supported)")
        }
        "文件 {file_path} 校验失败（CRC 不匹配），工程可能已损坏" => Some(
            "File {file_path} failed verification (CRC mismatch); the project may be corrupted",
        ),
        "文件 {file_path} 大小与文件头不一致（期望 {expected}，实际 {actual}），工程可能已损坏" => {
            Some(
                "File {file_path} size does not match the header (expected {expected}, got {actual}); the project may be corrupted",
            )
        }
        // ── crates/core/project/src/project/atomic.rs（原子保存失败提示）──
        "无效的保存路径: {path}" => Some("Invalid save path: {path}"),
        "保存目标为只读，无法覆盖：{path}（请先取消只读属性）" => Some(
            "The save target is read-only and cannot be overwritten: {path} (clear the read-only attribute first)",
        ),
        "无法清理旧备份 {path}：{e}" => Some("Could not remove the old backup {path}: {e}"),
        "无法备份原文件到 {path}：{e}" => {
            Some("Could not back up the original file to {path}: {e}")
        }
        "换入 {path} 失败：{e}；回滚失败：{rb}（原文件保留在 {bak}）" => Some(
            "Failed to swap in {path}: {e}; rollback also failed: {rb} (the original file is kept at {bak})",
        ),
        // ── crates/core/project/src/project/load.rs（工程格式识别失败提示）──
        "不是有效的 Lumino 工程文件（缺少 LMPJ 魔数）" => {
            Some("Not a valid Lumino project file (missing the LMPJ magic number)")
        }
        "不是有效的 Lumino 工程归档（缺少 LMPJ 魔数）" => {
            Some("Not a valid Lumino project archive (missing the LMPJ magic number)")
        }
        // ── crates/core/project/src/project/load/archive.rs（归档形态加载失败提示）──
        "归档读取失败: {e}" => Some("Failed to read the archive: {e}"),
        "归档中缺少 metadata.toml" => Some("metadata.toml is missing from the archive"),
        "metadata 编码错误: {e}" => Some("metadata encoding error: {e}"),
        "metadata 解析失败: {e}" => Some("Failed to parse metadata: {e}"),
        "读取音轨 {track_id} 失败: {e}" => Some("Failed to read track {track_id}: {e}"),
        "解码音轨 {track_id} 失败: {e}" => Some("Failed to decode track {track_id}: {e}"),
        "读取 tempo 失败: {e}" => Some("Failed to read tempo: {e}"),
        "tempo 解码失败: {e}" => Some("Failed to decode tempo: {e}"),
        "读取 signature 失败: {e}" => Some("Failed to read signature: {e}"),
        "signature 解码失败: {e}" => Some("Failed to decode signature: {e}"),
        "读取 controls 失败: {e}" => Some("Failed to read controls: {e}"),
        "controls 解码失败: {e}" => Some("Failed to decode controls: {e}"),
        "读取 aftertouch 失败: {e}" => Some("Failed to read aftertouch: {e}"),
        "aftertouch 解码失败: {e}" => Some("Failed to decode aftertouch: {e}"),
        "读取 text events 失败: {e}" => Some("Failed to read text events: {e}"),
        "text events 解码失败: {e}" => Some("Failed to decode text events: {e}"),
        "读取 text metas 失败: {e}" => Some("Failed to read text metas: {e}"),
        "text metas 解码失败: {e}" => Some("Failed to decode text metas: {e}"),
        "读取 SysEx 失败: {e}" => Some("Failed to read SysEx: {e}"),
        "SysEx 解码失败: {e}" => Some("Failed to decode SysEx: {e}"),
        "读取 names 失败: {e}" => Some("Failed to read names: {e}"),
        "names 解码失败: {e}" => Some("Failed to decode names: {e}"),
        // ── crates/core/project/src/project/load/folder.rs（文件夹形态加载失败提示）──
        "读取 metadata.toml 失败: {e}" => Some("Failed to read metadata.toml: {e}"),
        "读取音轨失败: {e}" => Some("Failed to read the tracks: {e}"),
        // ── crates/core/core/src/types.rs（NotePrecision / DotType / Language 显示名）──
        "全音符" => Some("Whole Note"),
        "二分音符" => Some("Half Note"),
        "四分音符" => Some("Quarter Note"),
        "八分音符" => Some("Eighth Note"),
        "十六分音符" => Some("Sixteenth Note"),
        "三十二分音符" => Some("Thirty-Second Note"),
        "六十四分音符" => Some("Sixty-Fourth Note"),
        "128分音符" => Some("128th Note"),
        "自定义" => Some("Custom"),
        "连音符" => Some("Tuplet"),
        "符点" => Some("Dotted"),
        "双符点" => Some("Double Dotted"),
        "（无）" => Some("(None)"),
        "简体中文" => Some("Simplified Chinese"),
        // ── crates/core/core/src/storage/config/enums.rs（设置下拉显示名）──
        "自动跳转到新音轨" => Some("Jump to the new track"),
        "保持当前音轨" => Some("Stay on the current track"),
        "弹簧动画" => Some("Spring animation"),
        "直接跟随" => Some("Direct follow"),
        "默认 (Shift+拖动框选)" => Some("Default (Shift+drag to box-select)"),
        "直接框选 (无需Shift)" => Some("Direct box-select (no Shift needed)"),
        "工程设置面板" => Some("Project settings panel"),
        "下边栏" => Some("Status bar"),
        "XSynth (内置)" => Some("XSynth (built-in)"),
        "系统 MIDI" => Some("System MIDI"),
        // ── crates/core/core/src/storage/config/autoscroll.rs（自动滚动模式显示名）──
        "固定指示线 (卷帘滚动)" => Some("Fixed indicator (scrolls the roll)"),
        "滚动指示线 (自动翻页)" => Some("Scrolling indicator (auto page flip)"),
        // 「关闭」在 ui_view / ui_shell 已登记为按钮译文，同 key 必须保持一致
        "关闭" => Some("Close"),
        // ── crates/core/message/src/events/window/audio.rs（音频导出下拉显示名）──
        "单声道" => Some("Mono"),
        "立体声" => Some("Stereo"),
        "自动" => Some("Auto"),
        "{n} 线程" => Some("{n} threads"),
        "无插值" => Some("No interpolation"),
        "线性插值" => Some("Linear interpolation"),
        "CPU (兼容)" => Some("CPU (compatible)"),
        "GPU (加速)" => Some("GPU (accelerated)"),
        _ => None,
    }
}
