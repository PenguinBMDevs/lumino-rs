# i18n 覆盖缺口审计报告

> 由 `scripts/i18n_audit.py` 自动生成，勿手工编辑。

## 总览

| 口径 | 条数 |
|---|---|
| 含中文的字符串字面量（含测试） | 9498 |
| 其中测试 / 基准 / 示例 | 5445 |
| 生产代码 | 4053 |
| **疑似用户可见文案（缺口主体）** | **277** |
| 涉及文件数（疑似文案） | 63 |

## 生产代码分类明细

| 分类 | 条数 | 是否算 i18n 缺口 |
|---|---:|---|
| `i18n-table` | 1304 | — 表自身 |
| `i18n-wrapped` | 1211 | ✅ 已覆盖（tr!/trf!） |
| `log` | 1066 | ❌ 否（开发者日志） |
| `ui` | 277 | ✅ 是 |
| `assert/panic` | 136 | ❌ 否 |
| `enum-parse` | 32 | ⚠️ 枚举解析错误串 |
| `print` | 16 | ❌ 否 |
| `expect` | 10 | ❌ 否 |
| `thiserror` | 1 | ⚠️ 属性宏，需人工重构 |

## 疑似文案按 crate 分布

| crate | 条数 |
|---|---:|
| `src` | 64 |
| `crates/editor/ui-core` | 51 |
| `crates/editor/ui` | 31 |
| `crates/editor/gfx` | 29 |
| `crates/core/message` | 25 |
| `crates/audio/midi-model` | 20 |
| `crates/tools/export` | 15 |
| `crates/tools/extras` | 13 |
| `crates/editor/dialog` | 9 |
| `crates/audio/midiplayer` | 5 |
| `crates/editor/ui-editor` | 5 |
| `crates/editor/ui-settings` | 4 |
| `crates/editor/editor-state` | 2 |
| `crates/audio/gpu-synth` | 1 |
| `crates/core/diagnostics` | 1 |
| `crates/network/cloud` | 1 |
| `crates/network/collaboration` | 1 |

## 疑似文案单文件 Top 40

| 文件 | 条数 |
|---|---:|
| `crates/editor/ui-core/src/button_descs.rs` | 29 |
| `crates/core/message/src/events/window/video/types.rs` | 20 |
| `src/runner/menu/window/dialog.rs` | 15 |
| `src/runner/menu/window/video_export/counter_font_ttf_load.rs` | 15 |
| `crates/editor/gfx/src/global_bucket/cull/extract.rs` | 13 |
| `crates/audio/midi-model/src/clipboard/domino.rs` | 12 |
| `crates/tools/export/src/video/config.rs` | 10 |
| `crates/audio/midi-model/src/clipboard.rs` | 8 |
| `crates/editor/gfx/src/global_bucket/cull/setup.rs` | 8 |
| `crates/editor/ui-core/src/state/video_export_state.rs` | 8 |
| `crates/editor/ui/src/view/video_export_dialog/counter_settings_font.rs` | 8 |
| `crates/tools/extras/build.rs` | 7 |
| `crates/tools/extras/src/palette/png.rs` | 6 |
| `src/runner/menu/window/video_export_handler/streaming_task.rs` | 6 |
| `src/runner/midi_manager/xsynth.rs` | 6 |
| `crates/audio/midiplayer/src/texture_waterfall/cache/io.rs` | 5 |
| `crates/editor/gfx/src/miditrail_renderer/cull/window.rs` | 5 |
| `crates/editor/ui-core/src/state/echo_cave.rs` | 5 |
| `crates/editor/ui/src/view/video_export_dialog/render_settings.rs` | 5 |
| `crates/core/message/src/events/window/video/types/render_mode.rs` | 4 |
| `crates/editor/dialog/src/platform/windows.rs` | 4 |
| `crates/editor/dialog/src/window/initialize.rs` | 4 |
| `crates/editor/ui-core/src/sidebar_event/group.rs` | 4 |
| `crates/editor/ui-core/src/sidebar_event/route.rs` | 4 |
| `crates/editor/ui-settings/src/pages/audio/types.rs` | 4 |
| `crates/editor/ui/src/right_sidebar/piano_waterfall/keyboard_renderer/shaders.rs` | 4 |
| `crates/editor/ui/src/root/handlers/toolbar/tools/note_ops.rs` | 4 |
| `src/runner/menu/window/video_export_handler/memory_task/task.rs` | 4 |
| `src/runner/menu/window/video_export/counter_font.rs` | 3 |
| `src/services/collaboration_service.rs` | 3 |
| `crates/editor/editor-state/src/editor_state/editor_data/async_commit.rs` | 2 |
| `crates/editor/ui-editor/src/edit_view.rs` | 2 |
| `crates/editor/ui-editor/src/impls/editor_impl/history.rs` | 2 |
| `crates/editor/ui/src/root/handlers/dialog/video_export/helpers.rs` | 2 |
| `crates/editor/ui/src/root/handlers/toolbar/tools/note_group_ops.rs` | 2 |
| `crates/editor/ui/src/view/video_export_dialog/data_curve_settings_style.rs` | 2 |
| `crates/tools/export/src/audio/render_loops.rs` | 2 |
| `crates/tools/export/src/audio/report.rs` | 2 |
| `src/runner/device_warning.rs` | 2 |
| `src/runner/menu/window/audio_export.rs` | 2 |

## 全部疑似文案明细


### `crates/audio/gpu-synth/src/synth/engine/render.rs`

- L131 | [SLOW-BLOCK] total={}us（预算 {}us）voices={} apply={}us collect={}us sync={}us upload={}us samples={}us mix={}us dispatch={}us readback={}us

### `crates/audio/midi-model/src/clipboard.rs`

- L91 | clipboard: varint 越界
- L99 | clipboard: varint 超长
- L109 | clipboard: 字节越界
- L118 | clipboard: u16 越界
- L121 | clipboard: u16 越界
- L213 | clipboard: 载荷过短
- L216 | clipboard: 魔数不匹配
- L220 | clipboard: 不支持的版本 {version}

### `crates/audio/midi-model/src/clipboard/domino.rs`

- L50 | u32 读取越界：长度 {}
- L70 | 分块长度溢出 id=0x{id:02x} kind=0x{kind:02x} len={len} pos={pos}
- L74 | 分块长度越界 id=0x{id:02x} kind=0x{kind:02x} len={len} pos={pos} buflen={}
- L83 | 分块流未完整消费：剩余 {}
- L118 | zlib 解压失败：{e}
- L121 | zlib 解压输出超过上限（{MAX_DECOMPRESSED_BYTES} 字节），已拒绝
- L138 | zlib 压缩写入失败：{e}
- L139 | zlib 压缩结束失败：{e}
- L147 | 不是 Domino PortalSequenceData 剪贴板数据
- L155 | 缺少 eb(0xeb) 音符容器块
- L191 | Domino 数据中未找到可解析的音符记录
- L211 | 模板缺少 eb 块

### `crates/audio/midiplayer/src/texture_waterfall/cache/io.rs`

- L106 | meta 长度 {meta_len} 超过上限 {MAX_META_BYTES}，缓存已损坏
- L123 | 缓存元数据 {meta:?} 与期望规格 (w={},h={},key={},ppq={},mpg={}) 不符
- L139 | 缓存压缩体超过上限 {MAX_COMPRESSED_BYTES} 字节
- L156 | 像素长度 {} 与规格 {}x{}x4={} 不符，缓存已损坏
- L177 | 解压像素超过上限 {MAX_TILE_BYTES} 字节

### `crates/core/diagnostics/src/memory_monitor/background.rs`

- L59 | MemoryMonitor: 内存已达 95% 软限制，主动终止以保护系统稳定

### `crates/core/message/src/events/window/video.rs`

- L181 | 未知的 MidiConsole 渲染后端: {other}

### `crates/core/message/src/events/window/video/types.rs`

- L193 | NPS（每秒音符数）
- L194 | 复音数
- L195 | 累计音符数
- L196 | BPM（速度）
- L243 | 左上
- L244 | 右上
- L245 | 左下
- L246 | 右下
- L247 | 顶部分散
- L248 | 底部分散
- L285 | 逗号
- L286 | 无
- L332 | 内置点阵（5x7）
- L333 | 系统字体：{family}
- L334 | 自定义字体：{path}
- L343 | 内置点阵
- L344 | 系统字体
- L345 | 微软雅黑
- L347 | 自定义字体
- L350 | 未知字体来源: {input}

### `crates/core/message/src/events/window/video/types/render_mode.rs`

- L55 | Lumino瀑布流
- L56 | Lumino卷帘
- L58 | 计数器
- L59 | 数据曲线

### `crates/editor/dialog/src/platform/windows.rs`

- L123 | 获取窗口句柄失败: {e}
- L126 | 不是 Windows 窗口
- L137 | 设置窗口过程失败
- L141 | 窗口过程表锁已损坏

### `crates/editor/dialog/src/window.rs`

- L117 | 创建对话框窗口失败: {e}

### `crates/editor/dialog/src/window/initialize.rs`

- L22 | 窗口大小为零，无法初始化
- L30 | 初始化图形上下文失败: {e}
- L53 | 窗口大小为零，无法初始化
- L56 | GFX 未初始化

### `crates/editor/editor-state/src/editor_state/editor_data/async_commit.rs`

- L55 | 已存在 pending commit，无法启动新的异步提交
- L157 | 异步提交线程异常断开

### `crates/editor/gfx/src/global_bucket/cull/extract.rs`

- L75 | cull 参数缓冲
- L87 | cull 管线
- L89 | cull 绑定组
- L96 | cull 计数缓冲
- L101 | cull 回读暂存
- L108 | cull 活跃键缓冲
- L113 | cull 活跃键暂存
- L123 | cull 回读暂存
- L141 | cull 活跃键暂存
- L188 | cull 基址缓冲
- L204 | cull 参数缓冲
- L211 | cull 管线
- L213 | cull 绑定组

### `crates/editor/gfx/src/global_bucket/cull/setup.rs`

- L177 | cull 布局
- L178 | cull 全局桶
- L182 | cull 参数缓冲
- L186 | cull 紧凑缓冲
- L190 | cull 计数缓冲
- L191 | cull 基址缓冲
- L230 | cull 游标缓冲
- L239 | cull 活跃键缓冲

### `crates/editor/gfx/src/grid_renderer.rs`

- L81 | 不再使用 CPU 实例生成网格线，GridRenderer 改用 GPU infinite grid 方案

### `crates/editor/gfx/src/miditrail_renderer/cull/window.rs`

- L25 | miditrail 常驻缓冲
- L96 | miditrail 常驻缓冲
- L132 | miditrail 紧凑缓冲
- L139 | miditrail 回读暂存
- L146 | miditrail 回读暂存

### `crates/editor/gfx/src/note_renderer/prepare.rs`

- L142 | 请改用 prepare_notes()

### `crates/editor/gfx/src/render_thread/render_loop/runner/video_export/waterfall.rs`

- L172 | 瀑布流

### `crates/editor/ui-core/src/button_descs.rs`

- L121 | 快退到起点
- L122 | 开始播放
- L123 | 暂停播放
- L124 | 快进到末尾
- L125 | 开始录制 MIDI 输入
- L126 | 停止录制
- L127 | 选择/移动音符
- L130 | Y向框选工具（Y维度自动全选，X维度按精度框选）
- L132 | 绘制音符
- L133 | 擦除音符
- L134 | 绘制自动化曲线
- L137 | 颜料桶：填充曲线围成的封闭区域为实心（仅曲线工具激活时可用）
- L139 | 将选中音符量化到网格
- L140 | 变速（按ctrl点击打开面板）
- L141 | 垂直翻转选中音符
- L142 | 水平翻转选中音符
- L143 | 分割音符（对齐演奏指示线分割）
- L144 | 合并选中的相邻音符
- L145 | 连奏选中音符
- L146 | 下移选中音符音高（按住ctrl进行八度移动）
- L147 | 上移选中音符音高（按住ctrl进行八度移动）
- L148 | 撤销上一步操作
- L149 | 重做已撤销操作
- L150 | 打开循环播放区域
- L151 | 切换自动滚动模式
- L152 | 打开多人协作面板
- L153 | 打开更多工具菜单
- L154 | 图片转MIDI（功能开发中）
- L157 | 开合音符画工具箱（鼠标/颜料桶/画刷/形状/文字）

### `crates/editor/ui-core/src/sidebar_event/group.rs`

- L46 | 钢琴卷帘组
- L50 | 工程走带
- L54 | 播放器
- L58 | 渲染器

### `crates/editor/ui-core/src/sidebar_event/route.rs`

- L29 | 视频渲染
- L33 | 音频渲染
- L62 | 横向卷帘
- L66 | 纵向卷帘

### `crates/editor/ui-core/src/state/echo_cave.rs`

- L28 | 你发现了回声洞。
- L29 | 喂——！……（回音：喂——！）
- L30 | 这里什么都没有，只有回声。
- L31 | 每一个音符，都有回音。
- L32 | 企鹅路过。🐧

### `crates/editor/ui-core/src/state/video_export_state.rs`

- L209 | 中
- L210 | Lumino瀑布流
- L220 | 左上
- L222 | 内置点阵
- L223 | 微软雅黑
- L230 | NPS（每秒音符数）
- L242 | 内置点阵
- L243 | 微软雅黑

### `crates/editor/ui-core/src/theme.rs`

- L18 | 高对比度 (High Contrast)

### `crates/editor/ui-editor/src/edit_view.rs`

- L65 | 钢琴卷帘
- L66 | 工程走带

### `crates/editor/ui-editor/src/impls/editor_impl/history.rs`

- L104 | 撤销
- L227 | 重做

### `crates/editor/ui-editor/src/interaction/moved.rs`

- L196 | 拖动变化

### `crates/editor/ui-settings/src/pages/audio/types.rs`

- L34 | 内置合成器
- L39 | 系统 MIDI
- L106 | 3 秒平均
- L108 | 瞬时

### `crates/editor/ui/src/host/dialog/state.rs`

- L264 | 已中止

### `crates/editor/ui/src/right_sidebar/piano_waterfall/keyboard_renderer/shaders.rs`

- L8 | \nstruct VsOut {\n    @builtin(position) pos: vec4<f32>,\n    @location(0) color: vec4<f32>,\n};\n\n// 活跃键颜色（复用卷帘瀑布流的 0xRRGGBBAA 打包：与 gfx waterfall.wgsl 一致）\n@group(0) @binding(0) var<storage, read> key_colors: array<u32>;\n\n// 解包 0xRRGGBBAA（R 在高字节，A 在低字节），与 gfx waterfall.wgsl 的 unpack_color 一致\nfn unpack_kc(packed: u32) -> vec4<u32> {\n    let r = (packed >> 24u) & 0xFFu;\n    let g = (packed >> 16u) & 0xFFu;\n    let b = (packed >> 8u) & 0xFFu;\n    let a = packed & 0xFFu;\n    return vec4<u32>(r, g, b, a);\n}\n\n// 复用卷帘瀑布流的 blend_key_color：base 为底色键色，overlay 为活跃色，alpha 控制混合强度\nfn blend_key_color(base: vec4<u32>, overlay: vec4<u32>, alpha: u32) -> vec4<u32> {\n    if (overlay.a == 0u || alpha == 0u) {\n        return base;\n    }\n    let a = alpha;\n    let r = (base.x * (255u - a) + overlay.x * a) / 255u;\n    let g = (base.y * (255u - a) + overlay.y * a) / 255u;\n    let b = (base.z * (255u - a) + overlay.z * a) / 255u;\n    return vec4<u32>(r, g, b, 255u);\n}\n\n@vertex\nfn vs(\n    @location(0) pos: vec2<f32>,\n    @location(1) rect: vec4<f32>,\n    @location(2) color: vec4<f32>,\n    @location(3) key: u32,\n) -> VsOut {\n    var o: VsOut;\n    o.pos = vec4<f32>(rect.xy + pos * rect.zw, 0.0, 1.0);\n\n    var out_color = color;\n    let ki = min(key, 255u);\n    let ac = key_colors[ki];\n    if (ac != 0u) {\n        let overlay = unpack_kc(ac);\n        let base8 = vec4<u32>(\n            u32(clamp(color.r, 0.0, 1.0) * 255.0),\n            u32(clamp(color.g, 0.0, 1.0) * 255.0),\n            u32(clamp(color.b, 0.0, 1.0) * 255.0),\n            255u,\n        );\n        let blended = blend_key_color(base8, overlay, overlay.a);\n        out_color = vec4<f32>(f32(blended.x), f32(blended.y), f32(blended.z), 255.0) / 255.0;\n    }\n    o.color = out_color;\n    return o;\n}\n\n@fragment\nfn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> {\n    return color;\n}\n
- L79 | \nstruct NoteInstance {\n    start_length: vec2<f32>,   // x = start_tick, y = length_tick\n    key_color: u32,            // 低 8 位 = key，高 24 位 = RGB（无 alpha）\n    border_width: u32,         // 高 16 位 = track_idx+1（主音轨判定）\n};\n\nstruct NoteUniforms {\n    resolution: vec2<f32>,     // 面板内容宽、全高\n    zoom_x: f32,               // 每 tick 像素数（与卷帘 X 缩放一致）\n    scroll_x: f32,             // 卷帘水平滚动（像素）\n    current_track: u32,        // 主音轨编码 = current_track_idx + 1\n    key_count: u32,\n    keyboard_y: f32,           // 键盘顶边 y（瀑布流落点线）\n    _pad: f32,\n};\n\n@group(0) @binding(0) var<storage, read> notes: array<NoteInstance>;\n@group(0) @binding(1) var<uniform> u: NoteUniforms;\n@group(0) @binding(2) var<storage, read> visible_indices: array<u32>;\n\nstruct VsOut {\n    @builtin(position) pos: vec4<f32>,\n    @location(0) color: vec4<f32>,\n};\n\nconst MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);\n\nfn unpack_key_color(packed: u32) -> vec4<f32> {\n    let rgb = packed >> 8u;\n    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;\n    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;\n    let b = f32(rgb & 0xFFu) / 255.0;\n    return vec4<f32>(r, g, b, 1.0);\n}\n\nfn is_black_key(k: i32) -> bool {\n    let m = k % 12;\n    return m == 1 || m == 3 || m == 6 || m == 8 || m == 10;\n}\n\n// 返回 [0, k) 范围内的白键数量（用于键位 x 定位）\n//\n// 阈值 {1,3,5,6,8,10} 为黑键音级：跨过一个黑键阈值，白键序号 +1。\n// 注意：B（rem=11）是**白键**，绝不能计入阈值——曾误加 `rem >= 11` 分支，\n// 导致每个八度的 B 调白键序号多算 1，瀑布流音符右移一个白键宽、错位到下一八度 C 列，\n// 表现为“B 调音符不显示”。此处仅统计真正落在 [0, rem) 的白键，故不含 rem=11。\nfn whites_before(k: i32) -> i32 {\n    let oct = k / 12;\n    let rem = k % 12;\n    var wb: i32 = 0;\n    if (rem >= 1) { wb = wb + 1; }\n    if (rem >= 3) { wb = wb + 1; }\n    if (rem >= 5) { wb = wb + 1; }\n    if (rem >= 6) { wb = wb + 1; }\n    if (rem >= 8) { wb = wb + 1; }\n    if (rem >= 10) { wb = wb + 1; }\n    return oct * 7 + wb;\n}\n\n@vertex\nfn vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VsOut {\n    var corners = array<vec2<f32>, 6>(\n        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),\n        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),\n    );\n    let cpos = corners[vid];\n\n    let real = visible_indices[iid];\n    let inst = notes[real];\n    let k = i32(inst.key_color & 0xFFu);\n    let white_count = whites_before(i32(u.key_count));\n    let white_w = u.resolution.x / f32(white_count);\n    let black_w = white_w * 0.58;\n    let wi = whites_before(k);\n\n    var x: f32;\n    var w: f32;\n    if (is_black_key(k)) {\n        x = f32(wi) * white_w - black_w * 0.5;\n        w = black_w;\n    } else {\n        x = f32(wi) * white_w;\n        w = white_w;\n    }\n\n    // y 轴：底部键盘线 = min_tick，后续 tick 向上（panel 顶部流入）\n    let start = inst.start_length.x;\n    let len = inst.start_length.y;\n    let y_top = u.keyboard_y - ((start + len) * u.zoom_x - u.scroll_x);\n    let y_bot = u.keyboard_y - (start * u.zoom_x - u.scroll_x);\n    let px_left = x;\n    let px_right = x + w;\n    let corner_x = select(px_left, px_right, cpos.x > 0.5);\n    let corner_y = select(y_top, y_bot, cpos.y > 0.5);\n\n    // 视口裁剪：完全在面板外则折叠为退化三角形（不栅格化）\n    var clip = vec2<f32>(2.0, 2.0);\n    if (!(y_bot < 0.0 || y_top > u.resolution.y || px_right < 0.0 || px_left > u.resolution.x)) {\n        clip = vec2<f32>(\n            corner_x / u.resolution.x * 2.0 - 1.0,\n            1.0 - corner_y / u.resolution.y * 2.0,\n        );\n    }\n\n    var out: VsOut;\n    out.pos = vec4<f32>(clip, 0.0, 1.0);\n\n    let track_enc = inst.border_width >> 16u;\n    var color = unpack_key_color(inst.key_color);\n    if (track_enc == u.current_track) {\n        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);\n    }\n    out.color = color;\n    return out;\n}\n\n@fragment\nfn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> {\n    return color;\n}\n
- L208 | \nstruct NoteInstance {\n    start_length: vec2<f32>,\n    key_color: u32,\n    border_width: u32,\n};\n\nstruct NoteUniforms {\n    resolution: vec2<f32>,\n    zoom_x: f32,\n    scroll_x: f32,\n    current_track: u32,\n    key_count: u32,\n    keyboard_y: f32,\n    _pad: f32,\n};\n\n@group(0) @binding(0) var<storage, read> notes: array<NoteInstance>;\n@group(0) @binding(1) var<uniform> u: NoteUniforms;\n@group(0) @binding(2) var<storage, read_write> key_colors: array<u32>;\n// 分块调度偏移：单次 dispatch 工作群组数上限 65535，超量音符需分块，每块带各自偏移\nstruct CullOffset { offset: u32, _p0: u32, _p1: u32, _p2: u32, };\n@group(0) @binding(3) var<uniform> cull_offset: CullOffset;\n\nconst MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);\n// 活跃键高亮强度（0..255），复用卷帘瀑布流 0xRRGGBBAA 打包\nconst ACTIVE_KEY_ALPHA: u32 = 200u;\n\nfn unpack_key_color(packed: u32) -> vec4<f32> {\n    let rgb = packed >> 8u;\n    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;\n    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;\n    let b = f32(rgb & 0xFFu) / 255.0;\n    return vec4<f32>(r, g, b, 1.0);\n}\n\nfn pack_kc(r: u32, g: u32, b: u32, a: u32) -> u32 {\n    return (r << 24u) | (g << 16u) | (b << 8u) | a;\n}\n\n@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n    let i = gid.x + cull_offset.offset;\n    if (i >= arrayLength(&notes)) { return; }\n    let inst = notes[i];\n    let k = inst.key_color & 0xFFu;\n    // 键盘线 tick：面板底部键盘线 = 当前滚动线（scroll_x / zoom_x）。\n    // 音符区间覆盖该线即正“落键”，点亮对应键。\n    let tk = u.scroll_x / u.zoom_x;\n    let start = inst.start_length.x;\n    let len = inst.start_length.y;\n    if (tk >= start && tk <= start + len) {\n        var col = unpack_key_color(inst.key_color);\n        if ((inst.border_width >> 16u) == u.current_track) {\n            col = vec4<f32>(MAIN_TRACK_COLOR, 1.0);\n        }\n        let r = u32(clamp(col.r, 0.0, 1.0) * 255.0);\n        let g = u32(clamp(col.g, 0.0, 1.0) * 255.0);\n        let b = u32(clamp(col.b, 0.0, 1.0) * 255.0);\n        key_colors[k] = pack_kc(r, g, b, ACTIVE_KEY_ALPHA);\n    }\n}\n
- L274 | \nstruct NoteInstance {\n    start_length: vec2<f32>,\n    key_color: u32,\n    border_width: u32,\n};\n\nstruct NoteUniforms {\n    resolution: vec2<f32>,\n    zoom_x: f32,\n    scroll_x: f32,\n    current_track: u32,\n    key_count: u32,\n    keyboard_y: f32,\n    _pad: f32,\n};\n\n@group(0) @binding(0) var<storage, read> notes: array<NoteInstance>;\n@group(0) @binding(1) var<uniform> u: NoteUniforms;\n@group(0) @binding(2) var<storage, read_write> visible_indices: array<u32>;\n// 间接绘制参数：[vertex_count, instance_count, first_vertex, first_instance]\n@group(0) @binding(3) var<storage, read_write> draw_args: array<atomic<u32>>;\n// 分块调度偏移：单次 dispatch 工作群组数上限 65535，超量音符需分块，每块带各自偏移\nstruct CullOffset { offset: u32, _p0: u32, _p1: u32, _p2: u32, };\n@group(0) @binding(4) var<uniform> cull_offset: CullOffset;\n\n@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n    let i = gid.x + cull_offset.offset;\n    if (i >= arrayLength(&notes)) { return; }\n    let inst = notes[i];\n    let start = inst.start_length.x;\n    let len = inst.start_length.y;\n    let y_top = u.keyboard_y - ((start + len) * u.zoom_x - u.scroll_x);\n    let y_bot = u.keyboard_y - (start * u.zoom_x - u.scroll_x);\n    if (y_bot >= -4.0 && y_top <= u.resolution.y + 4.0) {\n        let slot = atomicAdd(&draw_args[1], 1u);\n        visible_indices[slot] = i;\n    }\n}\n

### `crates/editor/ui/src/right_sidebar/piano_waterfall/waterfall_primitive.rs`

- L14 | \nstruct VSOut {\n    @builtin(position) pos: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n};\n\n@group(0) @binding(0) var tex: texture_2d<f32>;\n@group(0) @binding(1) var samp: sampler;\n\n@vertex\nfn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {\n    var positions = array<vec2<f32>, 6>(\n        vec2<f32>(-1.0, -1.0),\n        vec2<f32>( 1.0, -1.0),\n        vec2<f32>(-1.0,  1.0),\n        vec2<f32>(-1.0,  1.0),\n        vec2<f32>( 1.0, -1.0),\n        vec2<f32>( 1.0,  1.0),\n    );\n    var out: VSOut;\n    let p = positions[vi];\n    out.pos = vec4<f32>(p, 0.0, 1.0);\n    // 纹理行 0 为瀑布流顶部；clip y=+1 对应顶部 → 翻转到 uv.y\n    out.uv = vec2<f32>((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);\n    return out;\n}\n\n@fragment\nfn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {\n    return textureSample(tex, samp, uv);\n}\n

### `crates/editor/ui/src/root/handlers/dialog/audio_export.rs`

- L289 | 已中止

### `crates/editor/ui/src/root/handlers/dialog/video_export/helpers.rs`

- L93 | 系统字体
- L94 | 自定义字体

### `crates/editor/ui/src/root/handlers/toolbar/tools/note_group_ops.rs`

- L24 | 连奏
- L60 | 分割/合并

### `crates/editor/ui/src/root/handlers/toolbar/tools/note_ops.rs`

- L51 | 量化
- L253 | 垂直翻转
- L281 | 水平翻转
- L309 | 移调

### `crates/editor/ui/src/view/audio_export_dialog/buttons.rs`

- L136 | 已中止

### `crates/editor/ui/src/view/video_export_dialog/counter_settings_font.rs`

- L21 | 微软雅黑
- L22 | 微软雅黑粗体
- L23 | 宋体
- L24 | 黑体
- L25 | 楷体
- L26 | 仿宋
- L93 | 系统字体
- L100 | 自定义字体

### `crates/editor/ui/src/view/video_export_dialog/data_curve_settings_style.rs`

- L207 | 系统字体
- L214 | 自定义字体

### `crates/editor/ui/src/view/video_export_dialog/render_settings.rs`

- L54 | 计数器
- L64 | 数据曲线
- L141 | 高
- L141 | 中
- L141 | 低

### `crates/network/cloud/src/webdav/error.rs`

- L39 | <非 UTF-8 值>

### `crates/network/collaboration/src/types/client.rs`

- L34 | 用户{}

### `crates/tools/export/src/audio/render_loops.rs`

- L48 | 流式导出
- L118 | 内存导出

### `crates/tools/export/src/audio/report.rs`

- L26 | 轨{idx}:port{port}
- L29 | 等 +{}

### `crates/tools/export/src/project/loader.rs`

- L38 | 入口文件指向的数据文件夹不存在: {:?}

### `crates/tools/export/src/video/config.rs`

- L56 | 未知容器格式: {}
- L133 | 未知视频编码器: {}
- L231 | 未知编码后端: {}
- L274 | 高
- L275 | 中
- L276 | 低
- L286 | 高
- L287 | 中
- L288 | 低
- L289 | 未知质量预设: {}

### `crates/tools/extras/build.rs`

- L18 | 构建环境变量 {key} 未设置（cargo 应始终注入）
- L75 | // !!! 自动生成 - 请勿手动修改 !!!\n
- L76 | // 由 build.rs 在编译时根据 resources/palettes/ 目录生成\n\n
- L153 | // !!! 自动生成 - 请勿手动修改 !!!\n
- L154 | // 由 build.rs 在编译时根据 resources/Materials/ 目录生成\n\n
- L196 | // !!! 自动生成 - 请勿手动修改 !!!\n// 由 build.rs 生成（空目录）\n\n#[allow(unused_imports)]\nuse super::EmbeddedMaterial;\n\n&[]\n
- L213 | // !!! 自动生成 - 请勿手动修改 !!!\n// 由 build.rs 生成（空目录）\n\n#[allow(unused_imports)]\nuse super::EmbeddedPalette;\n\n&[]\n

### `crates/tools/extras/src/palette/png.rs`

- L17 | PNG 解码错误: {}
- L34 | 无法读取 PNG: {}
- L49 | 无法解码帧: {}
- L75 | 索引色 PNG 缺少 PLTE 块
- L100 | 不支持的 PNG 格式: color_type={:?}, bit_depth={:?}
- L109 | 调色板中没有颜色

### `src/cli.rs`

- L8 | Lumino 音乐编辑器 - 命令行测试模式

### `src/runner/device_warning.rs`

- L71 | 创建警告窗口失败: {e}
- L76 | 初始化警告窗图形上下文失败: {e}

### `src/runner/inner/persist.rs`

- L20 | (空)

### `src/runner/menu/file/export_flow.rs`

- L75 | {}：{e}

### `src/runner/menu/file/save_flow.rs`

- L53 | {}：{msg}

### `src/runner/menu/window/audio_export.rs`

- L70 | 内存模式（零拷贝）
- L72 | 文件模式

### `src/runner/menu/window/dialog.rs`

- L15 | 自定义精度
- L39 | 音符画设置
- L43 | 音符画设置
- L46 | 自定义精度
- L51 | 协作
- L52 | 协作
- L73 | 工程设置
- L94 | 音符变速
- L95 | 批量编辑
- L96 | 视频导出
- L98 | 内存监控
- L101 | 内存监控
- L112 | 音符变速
- L119 | 批量编辑
- L135 | 找回删除音轨

### `src/runner/menu/window/video_export/counter_font.rs`

- L167 | 内置点阵 5x7，字号 {}px（scale {}）
- L172 | 系统字体「{family}」，字号 {}px
- L175 | 自定义字体 {path}，字号 {}px

### `src/runner/menu/window/video_export/counter_font_ttf_load.rs`

- L17 | 微软雅黑
- L18 | 微软雅黑粗体
- L19 | 宋体
- L20 | 黑体
- L21 | 楷体
- L22 | 仿宋
- L31 | 微软雅黑
- L31 | 宋体
- L31 | 黑体
- L60 | 字体文件解析失败: {e}
- L66 | 内置点阵字体不需要 TTF 后端
- L69 | 未找到系统字体「{family}」
- L72 | 读取字体文件 {} 失败: {e}
- L77 | 未指定自定义字体文件路径
- L80 | 读取字体文件 {path} 失败: {e}

### `src/runner/menu/window/video_export_handler/memory_task/task.rs`

- L181 | 视频渲染
- L185 | 总时长 {:.1}s | 总帧数 {} | PPQ {}
- L317 | 已取消 | 已处理 {}/{} 帧 | 耗时 {:.1}s
- L322 | 完成 {}/{} 帧 | 耗时 {:.1}s

### `src/runner/menu/window/video_export_handler/pipeline.rs`

- L181 | 帧 {}/{} | FPS {:.0} | ETA {:.0}s | recv={} composite={} preview={} encode={}

### `src/runner/menu/window/video_export_handler/streaming_task.rs`

- L61 | MIDI解析
- L91 | 缓存就绪
- L108 | 视频渲染
- L112 | 总时长 {:.1}s | 总帧数 {} | PPQN {} | total_ticks {}
- L251 | 已取消 | 已处理 {}/{} 帧 | 耗时 {:.1}s
- L256 | 完成 {}/{} 帧 | 耗时 {:.1}s

### `src/runner/midi_manager/xsynth.rs`

- L82 | 初始化 MIDI API 失败: {:?}
- L100 | 开
- L102 | 关
- L111 | 获取输出设备失败: {:?}
- L113 | 未找到可用的 MIDI 输出设备
- L117 | 打开输出连接失败: {:?}

### `src/runner/window_manager.rs`

- L51 | 创建窗口失败: {e}
- L64 | 初始化图形上下文失败: {e}

### `src/services/collaboration_service.rs`

- L8 | 默认房间
- L10 | 协作客户端未初始化
- L12 | 协作客户端锁被污染

### `src/storage.rs`

- L124 | 无法创建项目目录
