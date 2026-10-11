//! 文案目录分域：`src/runner/{lifecycle,midi_manager,inner}**` 与宿主胶水层
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // ── 云存储：文件操作结果（cloud_files / cloud_ops） ──
        "操作成功" => Some("Operation succeeded"),
        "操作失败" => Some("Operation failed"),
        "加载失败：{err}" => Some("Load failed: {err}"),
        "正在下载 {file_name}" => Some("Downloading {file_name}"),
        "下载完成：{file_name}" => Some("Download complete: {file_name}"),
        "下载失败：{file_name}" => Some("Download failed: {file_name}"),
        "下载失败" => Some("Download failed"),
        "素材已导入素材库" => Some("Asset imported into the asset library"),
        "已下载并导入工程" => Some("Project downloaded and imported"),
        "已下载到 {path}" => Some("Downloaded to {path}"),
        // ── 云存储：保存 / 上传（cloud_save） ──
        "正在上传中，请稍候再试" => Some("Upload in progress, please try again later"),
        "当前没有可保存的工程内容" => Some("There is no project content to save"),
        "正在导出工程 {file_stem}.lmpj" => Some("Exporting project {file_stem}.lmpj"),
        "导出工程失败：{e}" => Some("Failed to export project: {e}"),
        "正在上传 {file_stem}.lmpj 到云存储" => {
            Some("Uploading {file_stem}.lmpj to cloud storage")
        }
        "已保存到云存储" => Some("Saved to cloud storage"),
        "保存到云失败：{err}" => Some("Failed to save to cloud: {err}"),
        "保存到云失败" => Some("Failed to save to cloud"),
        "保存失败" => Some("Save failed"),
        "未知错误" => Some("Unknown error"),
        "正在上传 {tmp_remote}" => Some("Uploading {tmp_remote}"),
        "上传完成，正在更新云端文件" => {
            Some("Upload complete, updating the cloud file")
        }
        "正在完成云端重命名" => Some("Finalizing the cloud rename"),
        "上传后重命名失败，文件已保留为 {tmp_remote}：{e}" => {
            Some("Rename failed after upload; the file was kept as {tmp_remote}: {e}")
        }
        "删除云端旧文件失败：{e}" => Some("Failed to delete the old cloud file: {e}"),
        "上传到云端失败：{e}" => Some("Failed to upload to the cloud: {e}"),
        "正在上传素材 {file_name}" => Some("Uploading asset {file_name}"),
        "素材 {file_name} 已上传到云存储" => {
            Some("Asset {file_name} uploaded to cloud storage")
        }
        "素材 {file_name} 上传失败：{err}" => {
            Some("Failed to upload asset {file_name}: {err}")
        }
        "素材已上传到云存储" => Some("Asset uploaded to cloud storage"),
        "素材上传失败" => Some("Asset upload failed"),
        // ── 云存储：连接与错误回显（cloud / cloud_status） ──
        "连接已删除" => Some("Connection deleted"),
        "云存储连接失败" => Some("Cloud storage connection failed"),
        "{context}（{e}）" => Some("{context} ({e})"),
        "{context}（未知错误）" => Some("{context} (unknown error)"),
        // ── 宿主窗口标题（进度窗 / 警告窗） ──
        "云存储传输" => Some("Cloud Storage Transfer"),
        "MIDI 处理进度" => Some("MIDI Processing Progress"),
        "GPU 兼容性检查" => Some("GPU Compatibility Check"),
        // ── GPU 兼容性：提示与失败详情（device_gate / device_warning） ──
        "未检测到 {backend} 支持，运行可能不稳定（可在设置中查看详情）" => {
            Some(
                "No support for {backend} detected; the app may be unstable (see Settings for details)",
            )
        }
        "放我进去" => Some("Let Me In Anyway"),
        "确认并关闭" => Some("Confirm and Close"),
        "未检测到支持 {backend} 的显卡或驱动，可尝试更新显卡驱动。" => Some(
            "No graphics card or driver supporting {backend} was detected. Try updating your graphics driver.",
        ),
        "检测到 {backend} 设备，但初始化失败（驱动可能不满足要求）。" => {
            Some(
                "A {backend} device was detected, but initialization failed (the driver may not meet the requirements).",
            )
        }
        "{backend} 渲染管线执行失败，请尝试更新显卡驱动。" => Some(
            "The {backend} render pipeline failed to execute. Try updating your graphics driver.",
        ),
        "{backend} 检测超时，驱动可能异常；已跳过本次检测。" => {
            Some("The {backend} check timed out; the driver may be faulty. This check was skipped.")
        }
        "{backend} 兼容性检测未通过。" => Some("{backend} compatibility check failed."),
        "{base}（检测到其他可用后端：{others}，可能仍可运行）" => {
            Some("{base} (other usable backends detected: {others}, the app may still run)")
        }
        // ── MIDI 后端回显（lifecycle/midi、midi_manager） ──
        "MIDI 播放输出创建失败，播放将无声：请检查输出设备（设置 → MIDI）后重试" => {
            Some(
                "Failed to create the MIDI playback output; playback will be silent. Check the output device (Settings → MIDI) and try again",
            )
        }
        "LGS 未生效：未设置音色库，当前使用 System 后端；请在设置中加载音色库（.sf2/.sfz）" => {
            Some(
                "LGS is inactive: no soundfont is configured, so the System backend is in use. Load a soundfont (.sf2/.sfz) in Settings",
            )
        }
        "LGS 未生效：音色库文件不存在（{path}），当前使用 System 后端" => {
            Some(
                "LGS is inactive: the soundfont file does not exist ({path}); the System backend is in use",
            )
        }
        "初始化 LGS (GPU) MIDI API 失败: {e}" => {
            Some("Failed to initialize the LGS (GPU) MIDI API: {e}")
        }
        "获取 LGS (GPU) 输出设备失败: {e}" => {
            Some("Failed to get the LGS (GPU) output devices: {e}")
        }
        "未找到可用的 LGS (GPU) MIDI 输出设备" => {
            Some("No usable LGS (GPU) MIDI output device was found")
        }
        "打开 LGS (GPU) 输出连接失败: {e}" => {
            Some("Failed to open the LGS (GPU) output connection: {e}")
        }
        "LGS 初始化失败，已回退 System 后端：{e}" => {
            Some("LGS initialization failed; fell back to the System backend: {e}")
        }
        "LGS 初始化线程异常断开，已回退 System 后端" => Some(
            "The LGS initialization thread disconnected unexpectedly; fell back to the System backend",
        ),
        // ── 文件导出进度（services/file_service） ──
        "准备导出 MIDI 文件" => Some("Preparing to export MIDI file"),
        "正在导出 MIDI 文件" => Some("Exporting MIDI file"),
        "MIDI 导出成功" => Some("MIDI export succeeded"),
        "正在写入文件" => Some("Writing file"),
        "写入文件失败: {e}" => Some("Failed to write file: {e}"),
        // ── 协作（services/collaboration_service） ──
        "协作通知" => Some("Collaboration Notice"),
        // ── 文件对话框过滤器标签（constants::filters；其余 5 个标签复用
        //    ui_shell 分域的同名登记） ──
        "压缩包文件" => Some("Archive Files"),
        "MIDI 与压缩包文件" => Some("MIDI and Archive Files"),
        _ => None,
    }
}
