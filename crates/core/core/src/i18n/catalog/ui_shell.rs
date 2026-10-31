//! 文案目录分域：crates/editor/ui/src/{root,toolbar,titlebar,sidebar,right_sidebar,host,mixer_panel}**（编辑器外壳）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        "（轨道列表占位）" => Some("(Track list placeholder)"),
        "（键盘渲染中或面板不可见）" => Some("(Keyboard rendering or panel hidden)"),
        "（瀑布流渲染中…）" => Some("(Rendering waterfall…)"),
        "{format} 文件" => Some("{format} Files"),
        "{name}预览" => Some("{name} Preview"),
        "{value}分音符" => Some("1/{value} note"),
        "八叉树" => Some("Octree"),
        "播放指示线不在视频带上（当前位置无素材）" => {
            Some("Playhead is outside the video strip (no material here)")
        }
        "不要再提示我" => Some("Don't remind me again"),
        "层次聚类" => Some("Hierarchical"),
        "处理中..." => Some("Processing..."),
        "导出设置（复用）" => Some("Export Settings (shared)"),
        "导出失败: {error}" => Some("Export failed: {error}"),
        "导出完成" => Some("Export complete"),
        "调色板" => Some("Palette"),
        "调色板生成失败: {e}" => Some("Palette generation failed: {e}"),
        "端口修改未应用：文档不可用" => {
            Some("Port change not applied: document unavailable")
        }
        "方差分割" => Some("Variance Split"),
        "房间: {room} | 邀请码: {code}" => Some("Room: {room} | Invite code: {code}"),
        "放我进去" => Some("Let Me In Anyway"),
        "分辨率: {width}x{height}  帧率: {fps}  速度: {speed}x" => {
            Some("Resolution: {width}x{height}  Frame rate: {fps}  Speed: {speed}x")
        }
        "服务器地址不能为空" => Some("Server address is required"),
        "复制目录暂不支持，请使用剪切移动" => {
            Some("Copying folders is not supported; use Cut to move")
        }
        "格式: {container}  编码: {codec}  质量: {quality}" => {
            Some("Format: {container}  Codec: {codec}  Quality: {quality}")
        }
        "固定位深调色板" => Some("Fixed Bit Palette"),
        "关闭" => Some("Close"),
        "轨道" => Some("Tracks"),
        "还原" => Some("Restore"),
        "绘制行为" => Some("Brush Behavior"),
        "混音台" => Some("Mixer"),
        "剪贴板为空" => Some("Clipboard is empty"),
        "没有可分享的形状，请先用鼠标工具选中已确认或待确认图形" => {
            Some(
                "No shareable shapes; select a confirmed or pending shape with the mouse tool first",
            )
        }
        "已复制形状分享文本" => Some("Shape share text copied"),
        "复制到剪贴板失败" => Some("Failed to copy to clipboard"),
        "形状分享编码失败" => Some("Failed to encode shape share text"),
        "已粘贴形状" => Some("Shapes pasted"),
        "形状分享粘贴失败" => Some("Failed to paste shape share text"),
        "读取剪贴板失败" => Some("Failed to read clipboard"),
        "矩形" => Some("Rectangle"),
        "均值漂移" => Some("Mean Shift"),
        "流式读取" => Some("Streaming"),
        "流行色" => Some("Popularity"),
        "每像素 tick" => Some("Ticks per Pixel"),
        "名称不能为空" => Some("Name is required"),
        "目标高度" => Some("Target Height"),
        "内存模式" => Some("In-memory"),
        "内置素材不可删除" => Some("Built-in materials cannot be deleted"),
        "内置素材不可上传" => Some("Built-in materials cannot be uploaded"),
        "内置素材不可重命名" => Some("Built-in materials cannot be renamed"),
        "你的电脑不满足运行 Lumino 的最低要求" => {
            Some("Your computer does not meet the minimum requirements to run Lumino")
        }
        "谱聚类" => Some("Spectral"),
        "瀑布流预览（等待渲染…）" => Some("Waterfall preview (waiting for render…)"),
        "瀑布流预览（纹理缺失）" => Some("Waterfall preview (texture missing)"),
        "请先完成当前编辑（拖动 / 绘制 / 调整大小）后再执行撤销/重做" => {
            Some("Finish the current edit (drag / draw / resize) before undoing or redoing")
        }
        "确认并关闭" => Some("Confirm and Close"),
        "三角形" => Some("Triangle"),
        "删除" => Some("Delete"),
        "上传到云" => Some("Upload to Cloud"),
        "设置端口" => Some("Set Port"),
        "设置通道" => Some("Set Channel"),
        "设置颜色" => Some("Set Color"),
        "视频剪辑" => Some("Video Clip"),
        "素材导入失败：不是有效的素材文件" => {
            Some("Import failed: not a valid material file")
        }
        "素材导入失败：复制文件出错" => Some("Import failed: could not copy the file"),
        "素材名称" => Some("Material Name"),
        "素材名称不能为空" => Some("Material name is required"),
        "素材删除失败：{e}" => Some("Failed to delete material: {e}"),
        "素材无效，无法上传" => Some("Invalid material, cannot upload"),
        "素材已保存为新名称，但旧文件删除失败：{e}" => {
            Some("Material saved under the new name, but the old file could not be deleted: {e}")
        }
        "素材已导入" => Some("Material imported"),
        "素材已删除" => Some("Material deleted"),
        "素材已重命名" => Some("Material renamed"),
        "素材重命名失败：{e}" => Some("Failed to rename material: {e}"),
        "素材重命名失败：无法读取原文件 {e}" => {
            Some("Failed to rename material: could not read the original file {e}")
        }
        "缩放 {zoom}x" => Some("Zoom {zoom}x"),
        "所有文件" => Some("All Files"),
        "通道修改未应用：文档不可用" => {
            Some("Channel change not applied: document unavailable")
        }
        "图片解码失败: {e}" => Some("Image decode failed: {e}"),
        "图片文件" => Some("Image Files"),
        "图片转 MIDI" => Some("Image to MIDI"),
        "未连接" => Some("Not connected"),
        "未配置" => Some("Not configured"),
        "未选择云存储" => Some("No cloud storage selected"),
        "未知素材" => Some("Unknown Material"),
        "文件夹名称不能为空" => Some("Folder name is required"),
        "无标题" => Some("Untitled"),
        "无效的文件名" => Some("Invalid file name"),
        "选择了一个图片文件" => Some("Image file selected"),
        "选择图片文件" => Some("Select Image"),
        "选择要导入的素材文件" => Some("Select Material File to Import"),
        "选择要转换为 MIDI 的图片" => Some("Select Image to Convert to MIDI"),
        "颜色数" => Some("Colors"),
        "已存在同名素材" => Some("A material with this name already exists"),
        "已复制：{name}" => Some("Copied: {name}"),
        "已剪切：{name}" => Some("Cut: {name}"),
        "已连接，请创建或加入房间" => Some("Connected. Create or join a room"),
        "已暂停" => Some("Paused"),
        "已中止" => Some("Aborted"),
        "音轨名称" => Some("Track Name"),
        "音乐文件" => Some("Music Files"),
        "音量 {value}" => Some("Volume {value}"),
        "音色库文件" => Some("Soundfont Files"),
        "原生 K-Means" => Some("Native K-Means"),
        "圆形" => Some("Circle"),
        "暂无音轨" => Some("No Tracks"),
        "找回删除音轨" => Some("Recover Deleted Tracks"),
        "正在初始化..." => Some("Initializing..."),
        "正在初始化... [{source}]" => Some("Initializing... [{source}]"),
        "正在继续..." => Some("Resuming..."),
        "正在连接..." => Some("Connecting..."),
        "正在中止..." => Some("Aborting..."),
        "重命名" => Some("Rename"),
        "主音量" => Some("Master Volume"),
        "转换参数" => Some("Conversion Settings"),
        "转换失败: {e}" => Some("Conversion failed: {e}"),
        "转换为 MIDI" => Some("Convert to MIDI"),
        "转换中..." => Some("Converting..."),
        "准备中..." => Some("Preparing..."),
        "字体文件" => Some("Font Files"),
        "最大化" => Some("Maximize"),
        "最小化" => Some("Minimize"),
        "Bayer 有序抖动" => Some("Bayer Ordered Dither"),
        "BMP 图片" => Some("BMP Images"),
        "CSV 数据文件" => Some("CSV Data Files"),
        "DBSCAN 密度" => Some("DBSCAN"),
        "Floyd-Steinberg 抖动" => Some("Floyd-Steinberg Dither"),
        "GIF 图片" => Some("GIF Images"),
        "GMM 混合模型" => Some("Gaussian Mixture (GMM)"),
        "JPEG 图片" => Some("JPEG Images"),
        "Key 范围" => Some("Key Range"),
        "Lumino 素材" => Some("Lumino Materials"),
        "Lumino 项目" => Some("Lumino Projects"),
        "MIDI 文件" => Some("MIDI Files"),
        "PCA 主成分" => Some("PCA"),
        "PNG 图片" => Some("PNG Images"),
        "SF2 文件" => Some("SF2 Files"),
        "SFZ 文件" => Some("SFZ Files"),
        "SVG 矢量图" => Some("SVG Vector Images"),
        "WebP 图片" => Some("WebP Images"),
        // 洋葱皮渲染后端切换（「加载 MIDI 后禁止切换」用户硬约束的 Toast 文案）
        "加载 MIDI 后不可切换洋葱皮渲染方式，请先关闭 MIDI 工程" => Some(
            "The onion skin renderer cannot be changed after loading MIDI; close the MIDI project first",
        ),
        "请先返回钢琴卷帘、关闭视频导出面板并停止导出后再切换 CPU 洋葱皮" => {
            Some(
                "Return to the piano roll, close the video export panel and stop exporting before switching to CPU onion skin",
            )
        }
        "已切回横向卷帘：CPU 洋葱皮不支持纵向卷帘" => Some(
            "Switched back to the horizontal roll: CPU onion skin does not support vertical roll",
        ),
        // 洋葱皮后端 = CPU 时各不兼容入口的拦截文案（Step 4b，入口一入口一条）
        "CPU 洋葱皮后端不支持纵向卷帘" => {
            Some("The CPU onion skin renderer does not support the vertical roll")
        }
        "CPU 洋葱皮后端不支持工程走带视图" => {
            Some("The CPU onion skin renderer does not support the arrangement view")
        }
        "CPU 洋葱皮后端不支持全屏瀑布流" => {
            Some("The CPU onion skin renderer does not support the full-screen waterfall")
        }
        "CPU 洋葱皮后端不支持钢琴瀑布流预览" => {
            Some("The CPU onion skin renderer does not support the piano waterfall preview")
        }
        "CPU 洋葱皮后端不支持视频导出" => {
            Some("The CPU onion skin renderer does not support video export")
        }
        "已关闭钢琴瀑布流预览面板：CPU 洋葱皮不支持该预览" => Some(
            "Closed the piano waterfall preview panel: CPU onion skin does not support this preview",
        ),
        _ => None,
    }
}
