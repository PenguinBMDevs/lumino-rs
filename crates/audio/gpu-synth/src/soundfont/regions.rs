use super::*;

impl SoundFont {
    pub(super) fn add_region(&mut self, region: &xsynth_soundfonts::sf2::Sf2Region) {
        // Deduplicate sample data per channel (stereo pairs get two ids).
        let sample_channels = region.sample.len() as u32;
        let sample_id = self.dedup_samples_channel(region.sample.first().cloned());
        let sample_id_r = if sample_channels == 2 {
            self.dedup_samples_channel(region.sample.get(1).cloned())
        } else {
            sample_id
        };

        // SF2 样本已被 `load_soundfont` 重采样到 `self.sample_rate`，位置也
        // 已转换到该域；这里记录实际数据率（不再是 region 的原生率）。
        let native_rate = self.sample_rate;
        for key in region.keyrange.clone() {
            for vel in region.velrange.clone() {
                // note_params applies the baked note-on modulators (velocity
                // -> attenuation, velocity -> filter cutoff, key -> env).
                let params = region.note_params(key, vel);

                let tuned_key_cents =
                    (key as f32 - region.root_key as f32) * region.scale_tuning as f32;
                let speed_mult = cents_factor(
                    tuned_key_cents
                        + region.fine_tune as f32
                        + region.coarse_tune as f32 * 100.0
                        + params.tune_cents,
                );

                let cutoff = if self.use_effects {
                    params.cutoff.filter(|c| *c >= 1.0)
                } else {
                    None
                };

                let pan = ((params.pan as f32 / 500.0) + 1.0) / 2.0;

                let zone = Zone {
                    sample_id,
                    sample_id_r,
                    channels: sample_channels,
                    volume: params.volume,
                    pan,
                    speed_mult,
                    cutoff,
                    resonance_db: params.resonance,
                    loop_mode: if region.loop_start == region.loop_end {
                        LoopMode::NoLoop
                    } else {
                        region.loop_mode
                    },
                    loop_start: region.loop_start,
                    loop_end: region.loop_end,
                    offset: region.offset,
                    sample_end: region.sample_end,
                    envelope: envelope_from_ampeg(&params.ampeg_envelope),
                    exclusive_class: region.exclusive_class,
                    native_rate,
                };

                let zone_id = self.zones.len() as u16;
                self.zones.push(zone);
                let idx = key as usize * 128 + vel as usize;
                self.zone_matrix[idx].push(zone_id);
            }
        }
    }

    pub(super) fn add_sfz_region(
        &mut self,
        region: &xsynth_soundfonts::sfz::RegionParams,
        sample_data: &[Arc<[f32]>],
        native_rate: u32,
    ) {
        // SFZ sample deduplication (mono 1 chan, stereo 2)
        let sample_channels = sample_data.len() as u32;
        let sample_id = self.dedup_samples_channel(sample_data.first().cloned());
        let sample_id_r = if sample_channels == 2 {
            self.dedup_samples_channel(sample_data.get(1).cloned())
        } else {
            sample_id
        };

        for key in region.keyrange.clone() {
            if key < 0 {
                continue;
            }
            let key_u8 = key as u8;
            for vel in region.velrange.clone() {
                let vel_u8 = vel;
                // SFZ pitch: keycenter + tune
                let speed_mult = {
                    // get_speed_mult_from_keys is private in xsynth, replicate cents_factor
                    let key_diff = key as f32 - region.pitch_keycenter as f32;
                    // SFZ tune is in cents, pitch_keycenter is midi note
                    cents_factor(key_diff * 100.0 + region.tune as f32)
                };
                // Envelope with vel2release
                let mut ampeg = region.ampeg_envelope.clone();
                ampeg.ampeg_release +=
                    (vel as f32 / 127.0) * region.ampeg_envelope.ampeg_vel2release;

                let cutoff = if self.use_effects {
                    region.cutoff.and_then(|mut c| {
                        if c < 1.0 {
                            return None;
                        }
                        // SFZ fil_veltrack / fil_keytrack modulation
                        let cents = vel as f32 / 127.0 * region.fil_veltrack as f32
                            + (key as f32 - region.fil_keycenter as f32)
                                * region.fil_keytrack as f32;
                        c *= cents_factor(cents);
                        Some(c.clamp(1.0, 20000.0))
                    })
                } else {
                    None
                };

                let pan_vel = vel as f32 / 127.0 * region.pan_veltrack
                    + (key as f32 - region.pan_keycenter as f32) * region.pan_keytrack;
                let pan_raw = (region.pan as f32 + pan_vel).clamp(-100.0, 100.0) / 100.0;
                let pan = (pan_raw + 1.0) / 2.0;

                let vol_vel = {
                    let a = region.amp_veltrack / 100.0;
                    let aabs = a.abs();
                    let v = vel as f32;
                    127.0 * (1.0 - aabs) + v * (a + aabs) / 2.0 + (127.0 - v) * (aabs - a) / 2.0
                };
                let vol_mult = (vol_vel / 127.0).powi(2);
                let vol_db_add = (key as f32 - region.amp_keycenter as f32) * region.amp_keytrack;
                let vol_db = (region.volume as f32 + vol_db_add).clamp(-96.0, 12.0);
                // db_to_amp helper: 10^(db/20)
                let volume = vol_mult * 10f32.powf(vol_db / 20.0);

                let zone = Zone {
                    sample_id,
                    sample_id_r,
                    channels: sample_channels,
                    volume,
                    pan,
                    speed_mult,
                    cutoff,
                    resonance_db: region.resonance,
                    loop_mode: if region.loop_start == region.loop_end {
                        LoopMode::NoLoop
                    } else {
                        region.loop_mode
                    },
                    loop_start: region.loop_start,
                    loop_end: region.loop_end,
                    offset: region.offset,
                    sample_end: sample_data.first().map(|s| s.len() as u32).unwrap_or(0),
                    envelope: envelope_from_ampeg(&ampeg),
                    exclusive_class: None,
                    native_rate,
                };

                let zid = self.zones.len() as u16;
                self.zones.push(zone);
                let idx = key_u8 as usize * 128 + vel_u8 as usize;
                self.zone_matrix[idx].push(zid);
            }
        }
    }

    fn dedup_samples_channel(&mut self, sample: Option<Arc<[f32]>>) -> usize {
        if let Some(sample) = sample {
            let ptr = Arc::as_ptr(&sample) as *const () as usize as u64;
            if let Some(&id) = self.sample_ids.get(&ptr) {
                return id;
            }
            let id = self.samples.len();
            self.samples.push(sample);
            self.sample_ids.insert(ptr, id);
            return id;
        }
        usize::MAX
    }
}

fn envelope_from_ampeg(p: &xsynth_soundfonts::sfz::AmpegEnvelopeParams) -> EnvelopeDescriptor {
    EnvelopeDescriptor {
        start_percent: p.ampeg_start / 100.0,
        delay: p.ampeg_delay,
        attack: p.ampeg_attack,
        hold: p.ampeg_hold,
        decay: p.ampeg_decay,
        sustain_percent: p.ampeg_sustain / 100.0,
        release: p.ampeg_release,
    }
}
