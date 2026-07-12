// Lumino GPU 音频渲染着色器
// 改编自 yinhe 的 voice_render.wgsl
// 每个工作线程处理一个 voice，将插值后的样本累加到输出缓冲区

// ===== GPU 缓冲区布局（std140 对齐） =====

// 常量缓冲区（每帧更新）
struct RenderParams {
    sample_rate: f32,
    num_voices: u32,
    num_samples: u32,
    output_offset: u32,
    max_voices: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

// 每个 voice 的状态（CPU 每帧更新，GPU 只读+写回）
struct VoiceState {
    sample_pos: f32,           // 亚样点位置
    pitch_ratio: f32,          // 变调比率
    volume: f32,               // 当前音量
    pan_left: f32,             // 左声道增益
    pan_right: f32,            // 右声道增益
    loop_start: f32,           // 循环起始（样点单位）
    loop_end: f32,             // 循环结束
    loop_mode: u32,            // 0=no_loop, 1=continuous
    sample_index: u32,         // 在 sample_data 中的偏移
    envelope_attack: f32,      // 包络：起音（秒）
    envelope_decay: f32,       // 包络：衰减（秒）
    envelope_sustain: f32,     // 包络：保持电平
    envelope_release: f32,     // 包络：释音（秒）
    envelope_value: f32,       // 当前包络值
    env_stage: u32,            // 0=attack, 1=decay, 2=sustain, 3=release, 4=finished
    env_time: f32,             // 当前阶段经过时间
    active: u32,               // 1=活跃, 0=空闲
    _pad: u32,                 // 16 字节对齐
};

// ===== 绑定组布局 =====

@group(0) @binding(0) var<uniform> params: RenderParams;
@group(0) @binding(1) var<storage, read_write> voice_states: array<VoiceState>;
@group(0) @binding(2) var<storage, read> sample_data: array<f32>;
@group(0) @binding(3) var<storage, read_write> output_buffer: array<f32>;

// ===== 工具函数 =====

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

// 计算包络值
fn compute_envelope(stage: ptr<function, u32>, env_value: ptr<function, f32>, env_time: ptr<function, f32>, delta_time: f32, attack: f32, decay: f32, sustain: f32, release: f32) {
    let s = *stage;
    var ev = *env_value;
    var t = *env_time + delta_time;

    if (s == 0u) {
        // Attack
        if (attack <= 0.001) {
            ev = 1.0;
            *stage = 1u;
            t = 0.0;
        } else {
            ev = min(1.0, t / attack);
            if (ev >= 1.0) {
                ev = 1.0;
                *stage = 1u;
                t = 0.0;
            }
        }
    }

    if (s == 1u) {
        // Decay
        if (decay <= 0.001) {
            *stage = 2u;
            t = 0.0;
        } else {
            ev = 1.0 - (1.0 - sustain) * min(1.0, t / decay);
            if (t >= decay) {
                ev = sustain;
                *stage = 2u;
                t = 0.0;
            }
        }
    }

    if (s == 2u) {
        // Sustain
        ev = sustain;
    }

    if (s == 3u) {
        // Release
        if (release <= 0.001) {
            ev = 0.0;
            *stage = 4u;
        } else {
            ev = ev * (1.0 - min(1.0, t / release));
            if (t >= release) {
                ev = 0.0;
                *stage = 4u;
            }
        }
    }

    *env_value = ev;
    *env_time = t;
}

// ===== 主计算入口 =====

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let voice_idx = id.x;
    if (voice_idx >= params.num_voices) {
        return;
    }

    var voice = voice_states[voice_idx];
    if (voice.active == 0u) {
        return;
    }

    let sample_rate = params.sample_rate;
    let num_samples = params.num_samples;
    let output_offset = params.output_offset;
    let delta_time = 1.0 / sample_rate;

    // 每个工作线程处理一个 voice 的完整样点序列
    for (var s: u32 = 0u; s < num_samples; s++) {
        // 更新包络
        compute_envelope(&voice.env_stage, &voice.envelope_value, &voice.env_time, delta_time,
                         voice.envelope_attack, voice.envelope_decay, voice.envelope_sustain, voice.envelope_release);

        // 如果 voice 已结束，跳出
        if (voice.env_stage == 4u) {
            break;
        }

        // 亚样点插值
        let index = u32(voice.sample_pos);
        let frac = voice.sample_pos - f32(index);

        // 读取左右声道样本（立体声交错）
        let base = voice.sample_index + index * 2u;
        let sample_left = lerp(sample_data[base], sample_data[base + 1u], frac);
        let sample_right = lerp(sample_data[base + 2u], sample_data[base + 3u], frac);

        // 应用音量和包络
        let gain = voice.volume * voice.envelope_value;
        let out_left = sample_left * gain * voice.pan_left;
        let out_right = sample_right * gain * voice.pan_right;

        // 累加到输出缓冲区（交错立体声）
        let out_idx = (output_offset + s) * 2u;
        output_buffer[out_idx] = output_buffer[out_idx] + out_left;
        output_buffer[out_idx + 1u] = output_buffer[out_idx + 1u] + out_right;

        // 前进样点位置
        voice.sample_pos = voice.sample_pos + voice.pitch_ratio;

        // 处理循环
        if (voice.loop_mode == 1u && voice.sample_pos >= voice.loop_end) {
            let loop_len = voice.loop_end - voice.loop_start;
            if (loop_len > 0.0) {
                voice.sample_pos = voice.sample_pos - loop_len;
            }
        }
    }

    // 将更新后的 voice 状态写回
    voice_states[voice_idx] = voice;
}