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
        "Lumino - 新一代MIDI编辑器" => Some("Lumino - Next-Generation MIDI Editor"),
        "版本 {version}" => Some("Version {version}"),
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
        // DRAW-041C：待确认组合（掩膜 / 竖切 / 补间 / 编辑）的状态与失败提示
        "编辑形状失败：请选中一个已 Bake 图形或单支待确认组合" => {
            Some("Edit shape failed: select a baked shape or a single pending composition")
        }
        "已切换遮罩（仅预览；确认后固定）" => {
            Some("Mask toggled (preview only; applied after confirmation)")
        }
        "遮罩失败：请选中一个待确认形状" => {
            Some("Mask failed: select a pending shape")
        }
        // 2026-11-05：DRAW-041C 遮罩/编辑图形的调用点用的是「图形」措辞，
        // 与上面「形状」条目不同字面 ⇒ 必须逐字登记，否则 i18n 覆盖率测试红灯。
        "已应用遮罩" => Some("Mask applied"),
        "应用遮罩失败：目标图形已不可编辑" => {
            Some("Failed to apply mask: the target shape is no longer editable")
        }
        "编辑图形" => Some("Edit Shape"),
        "编辑图形失败：请选中一个单支待确认组合（已 Bake 只读）" => Some(
            "Edit shape failed: select a single pending composition (baked shapes are read-only)",
        ),
        "已进入编辑图形：角=缩放，边=拉伸，角外=旋转，中心=移动" => Some(
            "Editing shape: corners = scale, edges = stretch, outside corner = rotate, center = move",
        ),
        "已退出编辑图形" => Some("Exited shape editing"),
        // DRAW-032：工程设置「启用通道 10」开关
        "启用通道 10" => Some("Enable channel 10"),
        "区域色号上限 16" => Some("Area colour max 16"),
        "区域着色 · 16 需启用通道 10" => Some("Area painting · 16 requires channel 10"),
        "遮罩设置…" => Some("Mask Settings…"),
        "遮罩设置失败：请选中一个待确认形状" => {
            Some("Failed to open mask settings: select a pending shape")
        }
        // DRAW-025：切割改为布防拖线；饼干模按「刀→面团」两个待确认形状
        "切割" => Some("Slice"),
        "饼干模" => Some("Mould"),
        // DEBT-14：抽屉插入 → 待确认 / 退回抽屉
        "退回抽屉" => Some("Return to drawer"),
        "已退回抽屉：可调整后重新 Insert" => {
            Some("Returned to drawer: adjust and insert again")
        }
        "退回失败：请选中一个从抽屉插入的待确认形状" => {
            Some("Return failed: select a pending shape inserted from the drawer")
        }
        "角度柔化" => Some("Round corners"),
        "角度柔化：拖角点调半径（Ctrl+拖调鼓度，Esc 退出）" => {
            Some("Round corners: drag a corner to set radius (Ctrl+drag sets bulge, Esc to exit)")
        }
        "角度柔化失败：请选中一个待确认折线形状（至少 3 个角点）" => {
            Some("Round corners failed: select a pending polyline shape with at least 3 corners")
        }
        "角度柔化 · 拖橙色角点调半径 · Esc 退出" => {
            Some("Round corners · drag an orange corner to set radius · Esc to exit")
        }
        "没有可柔化的角：先把中间节点拖出直线（Esc 退出）" => Some(
            "No roundable corner: drag a middle node off the straight line first (Esc to exit)",
        ),
        "拖出一条切割线：松手即切（Shift 自由角度）" => {
            Some("Drag a cut line; release to slice (Shift = free angle)")
        }
        "切割失败：请选中一个待确认形状/曲线；已 Bake 形状不可再切" => {
            Some("Slice failed: select a pending shape/curve; baked shapes cannot be sliced")
        }
        "已切出内/外片；确认后才写入音符" => {
            Some("Inner/outer pieces cut; notes are written after confirmation")
        }
        "以选中内容为刀" => Some("Use Selection as Knife"),
        "取消刀" => Some("Clear Knife"),
        // 右键菜单单竖列里的置灰项提示
        "该操作不适用于当前目标" => Some("Not available for this target"),
        "已取消刀" => Some("Knife cleared"),
        "当前没有刀" => Some("No knife is set"),
        "当前音轨没有可列出的已 Bake 形状" => {
            Some("No baked shapes to list on the current track")
        }
        "设刀失败：先选中音符或待确认形状" => {
            Some("Set knife failed: select notes or pending shapes first")
        }
        "已设为刀：{notes} 条音符 · {rows} 行（Esc 取消）" => {
            Some("Knife set: {notes} notes · {rows} rows (Esc to clear)")
        }
        "已设为刀：{rings} 环 · {rows} 行（Esc 取消）" => {
            Some("Knife set: {rings} rings · {rows} rows (Esc to clear)")
        }
        "刀：{notes} 条音符 · {rows} 行" => Some("Knife: {notes} notes · {rows} rows"),
        "刀：{rings} 环 · {rows} 行" => Some("Knife: {rings} rings · {rows} rows"),
        "刀在每一行都不足两段，切不出内片；请补一条同 key 音符或再加一个环" => {
            Some(
                "The knife has fewer than two segments on every row, so no inner piece can be cut; add a note on the same key or another ring",
            )
        }
        "饼干模失败：请选中 1 个待确认形状作为面团" => {
            Some("Mould failed: select exactly one pending shape as dough")
        }
        "饼干模失败：先右键「以选中内容为刀」设置刀" => {
            Some("Mould failed: set a knife via “Use Selection as Knife” first")
        }
        "饼干模失败：刀已失效（音符或形状已改变），请重新设刀" => {
            Some("Mould failed: the knife is stale (notes or shapes changed); set it again")
        }
        "刀已失效：音符或形状已改变，请重新设刀" => {
            Some("The knife is stale (notes or shapes changed); set it again")
        }
        "饼干模失败：刀与面团不能是同一个形状" => {
            Some("Mould failed: the knife and the dough cannot be the same shape")
        }
        "饼干模失败：刀的同一 key 行需要至少两段墙（中间才是空隙）" => {
            Some("Mould failed: each key row of the knife needs at least two wall segments")
        }
        "已生成补间（首 + 步 + 末）；确认后才写入音符" => {
            Some("Tween generated (first + step + last); notes are written after confirmation")
        }
        "补间失败：请恰好选中两个待确认形状/曲线" => {
            Some("Tween failed: select exactly two pending shapes/curves")
        }
        // DRAW-041C 阶段 3：020 显式手柄编辑的进入/退出提示
        "已进入手柄编辑；拖小圆点，Shift 关闭 15° 旋转吸附" => {
            Some("Handle edit mode on; drag the dots, Shift disables 15° rotation snapping")
        }
        "已退出手柄编辑" => Some("Handle edit mode off"),
        // DRAW-041C：组合入口拒绝必须给可见提示（混合选区 / 画刷 / 组合结果未 ✓）
        "转为活形状失败：请选中两个及以上待确认图形（已 Bake 只读）" => {
            Some(
                "Convert to live shape failed: select two or more pending shapes (baked shapes are read-only)",
            )
        }
        "连接图形失败：请选中至少两个待确认图形（已 Bake 只读）" => {
            Some("Join failed: select at least two pending shapes (baked shapes are read-only)")
        }
        "门控合并失败：请恰好选中两个待确认图形（已 Bake 只读）" => Some(
            "Gate merge failed: select exactly two pending shapes (baked shapes are read-only)",
        ),
        "沿路径复制失败：请选中一个待确认形状/曲线；组合结果需先确认" => {
            Some(
                "Copy along path failed: select one pending shape/curve; composed results must be confirmed first",
            )
        }
        "已应用力度渐强；确认后才写入音符" => {
            Some("Velocity ramp applied; notes are written after confirmation")
        }
        "力度渐强失败：请选中一个待确认形状/曲线；组合结果需先确认" => {
            Some(
                "Velocity ramp failed: select one pending shape/curve; composed results must be confirmed first",
            )
        }
        // DRAW-010：遮罩实体化入口
        "实体化" => Some("Bake to Shape"),
        "实体化遮罩" => Some("Bake Mask to Shape"),
        "已实体化遮罩：可见片段写成新形状" => {
            Some("Mask baked: visible pieces written as a new shape")
        }
        "实体化失败：请选中一个带遮罩的已 Bake 形状" => {
            Some("Bake mask failed: select a baked shape that carries a mask")
        }
        "已实体化遮罩" => Some("Mask baked to a new shape"),
        "实体化失败：遮罩没有可见片段或目标已失效" => {
            Some("Bake mask failed: no visible pieces or the target is no longer available")
        }
        // DRAW-014：三点圆弧工具
        "三点圆弧" => Some("Arc Tool (3 points)"),
        // DRAW-030：图片转 MIDI 参数面
        "图片降采样失败: {e}" => Some("Image downscale failed: {e}"),
        "观感调整" => Some("Look Adjustment"),
        "最长音符" => Some("Max Note Length"),
        "亮度" => Some("Brightness"),
        "对比度" => Some("Contrast"),
        "饱和度" => Some("Saturation"),
        "锐化" => Some("Sharpen"),
        "算法" => Some("Algorithms"),
        "混合模式" => Some("Blend Mode"),
        "key 映射" => Some("Key Mapping"),
        "音符长度" => Some("Note Length"),
        "重采样" => Some("Resampling"),
        "颜色匹配" => Some("Color Match"),
        "全部键" => Some("All Keys"),
        "白键（填充）" => Some("White Keys (Fill)"),
        "白键（裁切）" => Some("White Keys (Clip)"),
        "白键（固定列）" => Some("White Keys (Fixed)"),
        "黑键（填充）" => Some("Black Keys (Fill)"),
        "黑键（裁切）" => Some("Black Keys (Clip)"),
        "黑键（固定列）" => Some("Black Keys (Fixed)"),
        "不限" => Some("Unlimited"),
        "同色内切分" => Some("Split Within Color"),
        "按网格切分" => Some("Split to Grid"),
        "面积重采样" => Some("Area Resampling"),
        "最近邻" => Some("Nearest Neighbor"),
        "盒式" => Some("Box Filter"),
        "积分图" => Some("Integral Image"),
        "无（跟随调色板）" => Some("None (Use Palette)"),
        "扩散（误差）" => Some("Spread (Error Diffusion)"),
        "网点（4×4）" => Some("Pattern (4×4)"),
        "是否保留未保存的更改?" => Some("Keep unsaved changes?"),
        "删除素材" => Some("Delete Material"),
        "自定义 BPM 上限" => Some("Custom BPM Max"),
        "请输入 BPM 上限" => Some("Enter a BPM max"),
        "请输入大于 0 的数字" => Some("Enter a number greater than 0"),
        "上限越大，Tempo 曲线越平坦" => Some("Larger values flatten the Tempo curve"),
        "空格 播放/暂停" => Some("Space Play/Pause"),
        "跳过" => Some("Skip"),
        "（暂无子系统数据）" => Some("(No subsystem data)"),
        "此文件较大，加载可能需要一些时间…" => {
            Some("This file is large; loading may take a while…")
        }
        // ── 2026-11-20：按钮悬停说明补全（工具栏 / 侧栏 / 悬浮条 / 编辑器按钮）──
        "端口" => Some("Port"),
        "通道" => Some("Channel"),
        "精度" => Some("Precision"),
        "设置音符吸附到网格的精度" => Some("Set the grid precision notes snap to"),
        "查看并跳转到任意撤销步骤" => Some("View and jump to any undo step"),
        "点击编辑 BPM（当前拍速）" => Some("Click to edit BPM (current tempo)"),
        "点击编辑 PPQ（每四分音符的脉冲数）" => {
            Some("Click to edit PPQ (pulses per quarter note)")
        }
        "选择洋葱皮显示的轨范围" => Some("Choose which tracks the onion skin shows"),
        "洋葱皮范围" => Some("Onion Skin Range"),
        "打开绘制工具设置" => Some("Open drawing tool settings"),
        "拖动工具条" => Some("Drag the toolbar"),
        "确认当前绘制并生成音符" => Some("Confirm the current drawing and create notes"),
        "取消当前绘制" => Some("Cancel the current drawing"),
        "确认当前图形并生成音符" => Some("Confirm the current shape and create notes"),
        "取消当前图形" => Some("Cancel the current shape"),
        "确认放置并转换为 MIDI" => Some("Confirm placement and convert to MIDI"),
        "取消放置" => Some("Cancel placement"),
        "确认曲线并生成音符" => Some("Confirm the curve and create notes"),
        "取消当前曲线" => Some("Cancel the current curve"),
        "确认笔画并生成音符" => Some("Confirm the stroke and create notes"),
        "取消当前笔画" => Some("Cancel the current stroke"),
        "平滑曲线：锚点之间使用贝塞尔曲线连接" => {
            Some("Smooth curve: connect anchors with Bezier curves")
        }
        "折线：锚点之间用直线连接" => {
            Some("Polyline: connect anchors with straight lines")
        }
        "正常 / 合并模式：合并模式下连续文字会合并为一个音符组" => {
            Some("Normal / merge mode: in merge mode consecutive text becomes one note group")
        }
        "倒带到开头" => Some("Rewind to start"),
        "播放 / 暂停视频预览" => Some("Play / pause the video preview"),
        "只播放此音轨；再次点击取消" => Some("Solo this track; click again to cancel"),
        "点击查看内存使用明细" => Some("Click to view memory usage details"),
        "内存监控" => Some("Memory Monitor"),
        "恢复默认颜色" => Some("Restore default color"),
        "选择此颜色" => Some("Select this color"),
        "钢琴卷帘" => Some("Piano Roll"),
        "打开钢琴卷帘编辑视图" => Some("Open the piano roll editor"),
        "音轨总览" => Some("Track Overview"),
        "显示工程的全部音轨" => Some("Show all tracks in the project"),
        "自动化面板" => Some("Automation"),
        "显示当前音轨的自动化曲线" => {
            Some("Show the current track's automation curves")
        }
        "切换到工程走带 / 音轨总览视图" => {
            Some("Switch to the arrangement (track overview) view")
        }
        "瀑布流播放器" => Some("Waterfall Player"),
        "切换到瀑布流播放视图" => Some("Switch to the waterfall playback view"),
        "渲染" => Some("Render"),
        "打开渲染入口（视频 / 音频导出）" => {
            Some("Open the export entry (video / audio)")
        }
        "把工程渲染为视频文件" => Some("Render the project to a video file"),
        "把工程渲染为音频文件" => Some("Render the project to an audio file"),
        "横向卷帘" => Some("Horizontal Roll"),
        "钢琴卷帘横向显示（时间轴水平）" => {
            Some("Show the piano roll horizontally (time axis horizontal)")
        }
        "纵向卷帘" => Some("Vertical Roll"),
        "钢琴卷帘纵向显示（时间轴垂直）" => {
            Some("Show the piano roll vertically (time axis vertical)")
        }
        "打开 / 关闭混音台窗口" => Some("Open / close the mixer window"),
        "独奏" => Some("Solo"),
        "声像" => Some("Pan"),
        "声像：点击位置决定左右平衡" => {
            Some("Pan: click a position to set the left/right balance")
        }
        "拖动推子调整通道音量" => Some("Drag the fader to adjust the channel volume"),
        "静音此通道" => Some("Mute this channel"),
        "静音" => Some("Mute"),
        "音量" => Some("Volume"),
        "点击选择要转换的图片文件" => {
            Some("Click to choose the image to convert to MIDI")
        }
        "开始把图片转换为 MIDI" => Some("Start converting the image to MIDI"),
        "打开画刷行为设置" => Some("Open brush behavior settings"),
        _ => None,
    }
}
