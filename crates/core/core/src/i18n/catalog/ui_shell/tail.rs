//! `ui_shell.rs` 的后半段词条（按原 match 顺序切分）。

pub(super) fn lookup(zh: &str) -> Option<&'static str> {
    lookup_shell_6(zh)
        .or_else(|| lookup_shell_7(zh))
        .or_else(|| lookup_shell_8(zh))
        .or_else(|| lookup_shell_9(zh))
        .or_else(|| lookup_shell_10(zh))
}

fn lookup_shell_6(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_shell_7(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_shell_8(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_shell_9(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_shell_10(zh: &str) -> Option<&'static str> {
    match zh {
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
