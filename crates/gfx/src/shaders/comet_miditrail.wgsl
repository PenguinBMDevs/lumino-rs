// comet_miditrail.wgsl — MIDITrail 轨迹拖影风格（简化 GPU 实现）

@compute @workgroup_size(16, 16, 1)
fn miditrail_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
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
    // MIDITrail：当前 tick 位于约 1/3 处，前后都有可见内容
    let tick_start = tick - min(viewport_tick_span / 3u, 10000u);
    let tick_end = tick + (viewport_tick_span * 2u / 3u);
    let zoom_x = f32(w) / f32(key_count);
    let zoom_y = f32(note_area_h) / f32(viewport_tick_span);

    var color = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    if y < note_area_h {
        // ── 音符区域 + 拖影 ──
        let note_count = arrayLength(&notes);
        for (var i: u32 = 0u; i < note_count; i++) {
            let n = notes[i];
            if n.end_tick <= tick_start || n.start_tick >= tick_end {
                continue;
            }

            let note_x = u32(f32(n.key) * zoom_x);
            let note_w = u32(ceil(zoom_x));

            // 拖影：从 start_tick 到当前 tick
            var trail_top = note_area_h;
            var trail_bottom = note_area_h;
            if n.start_tick < tick {
                let trail_end = min(tick, n.end_tick);
                trail_top = u32(f32(tick_end - trail_end) * zoom_y);
                trail_bottom = u32(f32(tick_end - n.start_tick) * zoom_y);
            }
            // 音符头部
            let head_top = u32(f32(tick_end - min(n.end_tick, tick_end)) * zoom_y);
            let head_bottom = min(u32(f32(tick_end - n.start_tick) * zoom_y), note_area_h);

            let in_trail = x >= note_x && x < note_x + note_w && y >= trail_top && y < trail_bottom;
            let in_head = x >= note_x && x < note_x + note_w && y >= head_top && y < head_bottom;

            if in_trail || in_head {
                let age = f32(tick - n.start_tick);
                let total = f32(n.end_tick - n.start_tick);
                let fade = select(0.0, clamp(age / max(total, 1.0), 0.0, 0.8), tick > n.start_tick);
                let trail_alpha = 1.0 - fade;
                let vel_f = f32(n.velocity) / 127.0;
                let hue = f32(n.track_idx % 16u) / 16.0;
                let rgb = hsv_to_rgb(hue, 0.8, 0.4 + vel_f * 0.6);

                if in_head {
                    color = vec4<f32>(rgb, 0.9);
                } else {
                    color = vec4<f32>(rgb * trail_alpha * 0.7, trail_alpha * 0.8);
                }
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
             var base = vec4<f32>(0.92, 0.92, 0.92, 1.0);
             let key_active = active_keys[key_idx];
             if key_active != 0u {
                 let ac = unpack_color(key_active);
                base = mix(base, ac, 0.6);
            }
            color = base;
        }

         if black_key >= 0 {
             let key_idx = u32(black_key);
             var base = vec4<f32>(0.16, 0.16, 0.16, 1.0);
             let key_active = active_keys[key_idx];
             if key_active != 0u {
                 let ac = unpack_color(key_active);
                base = mix(base, ac, 0.6);
            }
            color = base;
        }
    }

    store_pixel(x, y, color);
}
