// comet_pfa.wgsl — PFA 侧视图钢琴卷帘风格（简化 GPU 实现）

@compute @workgroup_size(16, 16, 1)
fn pfa_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let w = params.frame_width;
    let h = params.frame_height;

    if x >= w || y >= h {
        return;
    }

    let key_count = params.key_count;
    let keyboard_width: f32 = 60.0;
    let content_width = f32(w) - keyboard_width;
    let ppq = params.ppq;
    let tick = params.tick;

    let viewport_tick_span = max(ppq * 16u, 1u);
    let zoom_x = content_width / f32(viewport_tick_span);
    let zoom_y = f32(h) / f32(key_count);

    var color = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    if x < u32(keyboard_width) {
        // ── 左侧键盘 ──
        let key = key_count - 1u - u32(f32(y) / zoom_y);
        if key < key_count {
             let key_active = active_keys[key];
             if is_black_key(key) {
                 color = vec4<f32>(0.16, 0.16, 0.16, 1.0);
                 if key_active != 0u {
                     color = vec4<f32>(0.78, 0.71, 0.59, 1.0);
                 }
             } else {
                 color = vec4<f32>(0.86, 0.86, 0.86, 1.0);
                 if key_active != 0u {
                     color = vec4<f32>(1.0, 0.86, 0.59, 1.0);
                 }
            }
        }
    } else {
        // ── 右侧音符区域 ──
        let note_count = arrayLength(&notes);
        let local_x = f32(x) - keyboard_width;
        let key = key_count - 1u - u32(f32(y) / zoom_y);

        if key < key_count {
            for (var i: u32 = 0u; i < note_count; i++) {
                let n = notes[i];
                if n.key != key {
                    continue;
                }
                let note_start_x = (f32(tick) - f32(n.start_tick)) * zoom_x;
                let note_end_x = (f32(tick) - f32(n.end_tick)) * zoom_x;
                if local_x >= note_end_x && local_x < note_start_x {
                    let vel_f = f32(n.velocity) / 127.0;
                    let rgb = unpack_color(n.color_packed).rgb * (0.5 + vel_f * 0.5);
                    color = vec4<f32>(rgb, 1.0);
                    break;
                }
            }
        }
    }

    store_pixel(x, y, color);
}
