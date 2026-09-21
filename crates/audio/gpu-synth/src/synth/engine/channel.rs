use super::*;

impl LerpState {
    pub(crate) fn new(initial: f32) -> Self {
        Self {
            frame: 0,
            current: initial,
            end: initial,
            step: 0.0,
        }
    }

    pub(crate) fn set_end(&mut self, end: f32, sample_rate: u32) {
        self.step = (end - self.current) / (sample_rate as f32 * 0.01);
        self.end = end;
    }

    /// Advances the lerp to the absolute `target` frame, returning the value
    /// at that point. Frame-exact, so the result does not depend on the
    /// block size.
    pub(crate) fn advance_to(&mut self, target: u64) -> f32 {
        let n = target.saturating_sub(self.frame);
        if n > 0 {
            self.frame = target;
            if self.end > self.current {
                self.current = (self.current + self.step * n as f32).min(self.end);
            } else if self.end < self.current {
                self.current = (self.current + self.step * n as f32).max(self.end);
            }
        }
        self.current
    }
}

impl ChannelState {
    pub(crate) fn new() -> Self {
        Self {
            program: 0,
            volume: LerpState::new(1.0),
            expression: LerpState::new(1.0),
            pan: LerpState::new(0.5),
            damper: false,
            pitch_multiplier: 1.0,
            env_attack: None,
            env_release: None,
            bend_value: 8192,
            bend_sensitivity: 2.0,
            fine_cents: 0.0,
            coarse_cents: 0.0,
            param: ParamSel::None,
            rpn0_msb: 2,
            rpn0_lsb: 0,
            rpn1_msb: 64,
            rpn1_lsb: 0,
        }
    }

    /// Recomputes `pitch_multiplier` from the current bend value/sensitivity
    /// and channel tuning (RPN 1/2).
    pub(crate) fn recompute_pitch(&mut self) {
        let bend_semitones = (self.bend_value as f32 - 8192.0) / 8192.0 * self.bend_sensitivity;
        let bend_mult = 2.0f32.powf(bend_semitones / 12.0);
        let tune_mult = 2.0f32.powf((self.fine_cents + self.coarse_cents) / 1200.0);
        self.pitch_multiplier = bend_mult * tune_mult;
    }

    /// 处理 RPN/NRPN 选择与 Data Entry（CC98/99/100/101/6/38）。
    ///
    /// MIDI 规范：CC100/CC101 = RPN LSB/MSB，CC98/CC99 = NRPN LSB/MSB，
    /// Data Entry 只作用于“当前选中的参数”。本引擎未实现任何 NRPN 效果，
    /// 因此 NRPN 选中期间的数据入口**消费丢弃**，绝不落到上一次 RPN 选择上。
    ///
    /// 返回是否可能改变音高（调用方据此触发 voice 的 pitch 传播）。
    pub(crate) fn handle_rpn_cc(&mut self, controller: u8, value: u8) -> bool {
        match controller {
            0x62 => {
                // CC98: NRPN LSB。
                let m = match self.param {
                    ParamSel::Nrpn(m, _) => m,
                    _ => 0,
                };
                self.param = ParamSel::Nrpn(m, value);
                false
            }
            0x63 => {
                // CC99: NRPN MSB。
                let l = match self.param {
                    ParamSel::Nrpn(_, l) => l,
                    _ => 0,
                };
                self.param = ParamSel::Nrpn(value, l);
                false
            }
            0x64 => {
                // CC100: RPN LSB。
                let m = match self.param {
                    ParamSel::Rpn(m, _) => m,
                    _ => 0,
                };
                self.param = ParamSel::Rpn(m, value);
                false
            }
            0x65 => {
                // CC101: RPN MSB。
                let l = match self.param {
                    ParamSel::Rpn(_, l) => l,
                    _ => 0,
                };
                self.param = ParamSel::Rpn(value, l);
                false
            }
            0x06 => {
                self.apply_data_entry(true, value);
                true
            }
            0x26 => {
                self.apply_data_entry(false, value);
                true
            }
            _ => false,
        }
    }

    /// 应用一个 Data Entry 字节（CC6 = MSB / CC38 = LSB）到当前选中的 RPN。
    ///
    /// 仅支持 RPN 0/1/2；`None` 与 NRPN 一律忽略（消费丢弃）。
    pub(crate) fn apply_data_entry(&mut self, is_msb: bool, value: u8) {
        match self.param {
            ParamSel::Rpn(0, 0) => {
                // Pitch Bend Sensitivity: MSB = 半音，LSB = 1/128 半音（128 精度）。
                if is_msb {
                    self.rpn0_msb = value;
                } else {
                    self.rpn0_lsb = value;
                }
                let semis = self.rpn0_msb as f32 + self.rpn0_lsb as f32 / 128.0;
                self.bend_sensitivity = semis.clamp(0.0, 96.0);
                self.recompute_pitch();
            }
            ParamSel::Rpn(0, 1) => {
                // Channel Fine Tuning: 标准 14-bit（MSB<<7|LSB），中心 8192 = 0 cents。
                if is_msb {
                    self.rpn1_msb = value;
                } else {
                    self.rpn1_lsb = value;
                }
                let raw = ((self.rpn1_msb as u16) << 7) | self.rpn1_lsb as u16;
                self.fine_cents = (raw as f32 - 8192.0) / 8192.0 * 100.0;
                self.recompute_pitch();
            }
            // Channel Coarse Tuning: 仅 MSB，64 = 0（LSB 不使用）。
            ParamSel::Rpn(0, 2) if is_msb => {
                self.coarse_cents = (value as f32 - 64.0) * 100.0;
                self.recompute_pitch();
            }
            _ => {}
        }
    }
}
