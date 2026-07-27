// comet_velocities.wgsl — Velocities 力度热力图

@compute @workgroup_size(16, 16, 1)
fn velocities_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let w = params.frame_width;
    let h = params.frame_height;

    if x >= w || y >= h {
        return;
    }

    let key_count = params.key_count;
    let cell_w = f32(w) / f32(key_count);
    let cell_h = f32(h) / 128.0;

    // 背景网格色
    var color = vec4<f32>(0.157, 0.157, 0.188, 1.0); // ~[40,40,48]

    let key = u32(f32(x) / cell_w);
    if key < key_count {
        // 查找该 key 当前是否有活跃音符，取最高力度
        var max_velocity: u32 = 0u;
        let note_count = arrayLength(&notes);
        for (var i: u32 = 0u; i < note_count; i++) {
            let n = notes[i];
            if n.key == key && note_is_active_at(n, params.tick) {
                if n.velocity > max_velocity {
                    max_velocity = n.velocity;
                }
            }
        }

        if max_velocity > 0u {
            let vel_f = f32(max_velocity) / 127.0;
            let brightness = 0.3 + vel_f * 0.7;
            // 热力图颜色：蓝 -> 青 -> 黄 -> 红
            let heat = hsv_to_rgb(0.66 - vel_f * 0.66, 1.0, brightness);
            color = vec4<f32>(heat, 1.0);
        }
    }

    store_pixel(x, y, color);
}
