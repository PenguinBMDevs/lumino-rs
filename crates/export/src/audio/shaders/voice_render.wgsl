// Lumino GPU 音频渲染着色器
// 每个工作线程处理一个 voice，将插值后的样本累加到输出缓冲区
//
// 注意：避免使用 ptr<function, T> 函数参数，naga 的 SPIR-V 后端
// 在复杂指针传递时存在 "Expression not cached" 内部错误。

// ===== GPU 缓冲区布局 =====

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

struct VoiceState {
    sample_pos: f32,
    pitch_ratio: f32,
    volume: f32,
    pan_left: f32,
    pan_right: f32,
    loop_start: f32,
    loop_end: f32,
    loop_mode: u32,
    sample_index: u32,
    envelope_attack: f32,
    envelope_decay: f32,
    envelope_sustain: f32,
    envelope_release: f32,
    envelope_value: f32,
    env_stage: u32,
    env_time: f32,
    is_active: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: RenderParams;
@group(0) @binding(1) var<storage, read_write> voice_states: array<VoiceState>;
@group(0) @binding(2) var<storage, read> sample_data: array<f32>;
@group(0) @binding(3) var<storage, read_write> output_buffer: array<f32>;

// ===== 工具函数 =====

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

// ===== 主计算入口 =====

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let voice_idx = id.x;
    if (voice_idx >= params.num_voices) {
        return;
    }

    var voice = voice_states[voice_idx];
    if (voice.is_active == 0u) {
        return;
    }

    let sample_rate = params.sample_rate;
    let num_samples = params.num_samples;
    let output_offset = params.output_offset;
    let delta_time = 1.0 / sample_rate;

    // 每个工作线程处理一个 voice 的完整样点序列
    for (var s: u32 = 0u; s < num_samples; s++) {
        // === 内联包络计算（避免 ptr<function> 函数参数，绕过 naga SPIR-V bug） ===
        let env_stage = voice.env_stage;
        var ev = voice.envelope_value;
        var et = voice.env_time + delta_time;
        var new_stage = env_stage;

        if (env_stage == 0u) {
            // Attack
            if (voice.envelope_attack <= 0.001) {
                ev = 1.0;
                new_stage = 1u;
                et = 0.0;
            } else {
                ev = min(1.0, et / voice.envelope_attack);
                if (ev >= 1.0) {
                    ev = 1.0;
                    new_stage = 1u;
                    et = 0.0;
                }
            }
        }

        if (env_stage == 1u) {
            // Decay
            if (voice.envelope_decay <= 0.001) {
                new_stage = 2u;
                et = 0.0;
            } else {
                ev = 1.0 - (1.0 - voice.envelope_sustain) * min(1.0, et / voice.envelope_decay);
                if (et >= voice.envelope_decay) {
                    ev = voice.envelope_sustain;
                    new_stage = 2u;
                    et = 0.0;
                }
            }
        }

        if (env_stage == 2u) {
            // Sustain
            ev = voice.envelope_sustain;
        }

        if (env_stage == 3u) {
            // Release
            if (voice.envelope_release <= 0.001) {
                ev = 0.0;
                new_stage = 4u;
            } else {
                ev = ev * (1.0 - min(1.0, et / voice.envelope_release));
                if (et >= voice.envelope_release) {
                    ev = 0.0;
                    new_stage = 4u;
                }
            }
        }

        voice.envelope_value = ev;
        voice.env_time = et;
        voice.env_stage = new_stage;

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