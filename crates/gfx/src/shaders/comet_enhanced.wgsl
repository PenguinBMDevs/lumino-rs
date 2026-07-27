// comet_enhanced.wgsl — Enhanced 3D 增强风格（简化 GPU 实现）

@compute @workgroup_size(16, 16, 1)
fn enhanced_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let w = params.frame_width;
    let h = params.frame_height;

    if x >= w || y >= h {
        return;
    }

    let key_count = params.key_count;
    let kb_height = params.kb_height;
    let note_area_h = h - min(h, kb_height);
    let ppq = params.ppq;
    let tick = params.tick;
    let speed = max(params.speed, 0.1);

    let ticks_per_measure = ppq * 4u;
    let visible_measure_count = max(u32(round(4.0 / speed)), 1u);
    let viewport_tick_span = max(ticks_per_measure * visible_measure_count, 1u);
    let tick_end = tick + viewport_tick_span;
    let zoom_x = f32(w) / f32(key_count);
    let zoom_y = f32(note_area_h) / f32(viewport_tick_span);

    var color = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    if y < note_area_h {
        // ── 音符区域 ──
        let note_count = arrayLength(&notes);
        for (var i: u32 = 0u; i < note_count; i++) {
            let n = notes[i];
            let visible_end = min(n.end_tick, tick_end);
            let visible_start = max(n.start_tick, tick);
            if visible_end <= visible_start {
                continue;
            }

            let note_x = u32(f32(n.key) * zoom_x);
            let note_w = u32(ceil(zoom_x));
            let note_top = u32(f32(tick_end - visible_end) * zoom_y);
            let note_bottom = min(u32(f32(tick_end - visible_start) * zoom_y), note_area_h);
            let note_h = max(note_bottom - note_top, 1u);

            if x >= note_x && x < note_x + note_w && y >= note_top && y < note_top + note_h {
                // 基础色 + 进度色相偏移
                let note_len = n.end_tick - n.start_tick;
                let progress = select(0.0, f32(tick - n.start_tick) / f32(note_len), note_len > 0u);
                let base_hue = f32(n.track_idx % 16u) / 16.0;
                let hue = (base_hue + progress * 0.3) % 1.0;
                let vel_f = f32(n.velocity) / 127.0;
                let rgb = hsv_to_rgb(hue, 0.8, 0.4 + vel_f * 0.6);
                color = vec4<f32>(rgb, 0.85);
                break;
            }
        }
    } else {
         // ── 键盘区域 ──
         let key_layout = key_layout_with_height(key_count, w, kb_height);
         let local_y = y - note_area_h;
         let xf = f32(x);
 
         let white_key = find_white_key_index(xf, key_count, key_layout);
         let black_key = find_black_key_index(xf, local_y, key_count, key_layout);

        if white_key >= 0 {
            let key_idx = u32(white_key);
            var base = vec4<f32>(0.92, 0.92, 0.92, 1.0); // 白键
            // 活跃键发光
            let active = active_keys[key_idx];
            if active != 0u {
                let ac = unpack_color(active);
                base = mix(base, ac, 0.5);
            }
            color = base;
        }

        if black_key >= 0 {
            let key_idx = u32(black_key);
            var base = vec4<f32>(0.16, 0.16, 0.16, 1.0); // 黑键
            let active = active_keys[key_idx];
            if active != 0u {
                let ac = unpack_color(active);
                base = mix(base, ac, 0.5);
            }
            color = base;
        }
    }

    store_pixel(x, y, color);
}
