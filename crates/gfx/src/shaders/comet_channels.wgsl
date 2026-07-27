// comet_channels.wgsl — Channels 通道热力图

@compute @workgroup_size(16, 16, 1)
fn channels_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let w = params.frame_width;
    let h = params.frame_height;

    if x >= w || y >= h {
        return;
    }

    let key_count = params.key_count;
    let channel_count: u32 = 16u;
    let cell_w = f32(w) / f32(key_count);
    let cell_h = f32(h) / f32(channel_count);

    // 背景
    var color = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    let key = u32(f32(x) / cell_w);
    let channel = u32(f32(h - 1u - y) / cell_h);

    if key < key_count && channel < channel_count {
        // 查找 (key, channel) 是否有活跃音符
        let note_count = arrayLength(&notes);
        var key_active: bool = false;
        for (var i: u32 = 0u; i < note_count && !key_active; i++) {
            let n = notes[i];
            if n.key == key && (n.channel % 16u) == channel && note_is_active_at(n, params.tick) {
                key_active = true;
            }
        }

        if key_active {
            let hue = f32(channel) / f32(channel_count);
            let rgb = hsv_to_rgb(hue, 0.7, 0.9);
            color = vec4<f32>(rgb, 1.0);
        }
    }

    store_pixel(x, y, color);
}
