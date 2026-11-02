//! REF-LGS-011: 共享包络模板池。
//!
//! 旧实现把每个声部的 stage 序列复制进 `upload_env_stages`（按声部前缀和
//! 排布，≈96 B/声部/块）。同音色同包络的成千上万个声部共享**完全相同**
//! 的 stage 序列，因此这里把序列去重成一份模板表，声部只保存起始下标
//! （[`super::Voice::env_template`]），表只在追加新模板时上传。
//!
//! 语义不变：模板里存的仍是 `(kind, target, duration)` 原值，求值仍在
//! `render.wgsl` 里按 stage 逐个推进（不改变任何浮点结果）。

use super::*;
use crate::synth::dsp::EnvStage;

/// 去重的包络模板池（append-only）。
#[derive(Debug, Default)]
pub(crate) struct EnvTemplatePool {
    /// 去重后的 stage 序列（模板 i 的起点是 `offsets[i]`）。
    stages: Vec<EnvStageGpu>,
    /// 序列哈希 -> `(起始下标, 长度)` 候选链（哈希碰撞时逐个比对）。
    index: std::collections::HashMap<u64, Vec<(u32, u32)>>,
    /// 已经写入 GPU 的 stage 数（只追加，故只需记录水位）。
    uploaded: usize,
}

impl EnvTemplatePool {
    /// 驻留一段 stage 序列，返回其起始下标。
    ///
    /// 相同序列（`kind/target_bits/duration` 全等）返回同一下标；模板池只
    /// 追加、不删除——模板数量由音色/包络变体数决定（通常个位数到几十），
    /// 与声部数无关。
    pub fn intern(&mut self, stages: &[EnvStage]) -> u32 {
        let hash = hash_stages(stages);
        if let Some(bucket) = self.index.get(&hash) {
            for &(off, len) in bucket {
                if len as usize == stages.len() {
                    let tail = &self.stages[off as usize..off as usize + len as usize];
                    if tail.iter().zip(stages).all(|(g, s)| {
                        g.kind == s.kind && g.target_val == s.target && g.duration == s.duration
                    }) {
                        return off;
                    }
                }
            }
        }
        let off = self.stages.len() as u32;
        self.stages.extend(stages.iter().map(|s| EnvStageGpu {
            kind: s.kind,
            target_val: s.target,
            duration: s.duration,
        }));
        self.index
            .entry(hash)
            .or_default()
            .push((off, stages.len() as u32));
        off
    }

    /// 驻留淡出模板（单 stage 线性衰到 0，REND-015 #115 的 5 ms 淡出）。
    ///
    /// 旧实现给每个淡出声部单独写一条 stage；现在它们共享同一条模板。
    pub fn intern_fade(&mut self, sample_rate: u32) -> u32 {
        let stage = EnvStage {
            kind: EnvStage::LERP,
            target: 0.0,
            duration: fade_frames(sample_rate) as u32,
        };
        self.intern(std::slice::from_ref(&stage))
    }

    /// 去重后的全部 stage（模板表）。
    pub fn stages(&self) -> &[EnvStageGpu] {
        &self.stages
    }

    /// 尚未上传到 GPU 的尾部（本块需要追加的部分）。
    pub fn pending(&self) -> &[EnvStageGpu] {
        &self.stages[self.uploaded..]
    }

    /// 标记尾部已经上传（下一个 `pending` 从新水位开始）。
    pub fn mark_uploaded(&mut self) {
        self.uploaded = self.stages.len();
    }
}

/// FNV-1a over `(len, kind, target_bits, duration)` per stage.
fn hash_stages(stages: &[EnvStage]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bits: u64| {
        h ^= bits;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(stages.len() as u64);
    for s in stages {
        mix(u64::from(s.kind));
        mix(u64::from(s.target.to_bits()));
        mix(u64::from(s.duration));
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(kind: u32, target: f32, duration: u32) -> EnvStage {
        EnvStage {
            kind,
            target,
            duration,
        }
    }

    /// 相同序列必须复用同一下标，不同序列必须分开驻留（bit-exact 前提）。
    #[test]
    fn intern_dedups_identical_sequences() {
        let mut pool = EnvTemplatePool::default();
        let a = [stage(0, 1.0, 100), stage(0, 0.5, 200)];
        let b = [stage(0, 1.0, 100), stage(0, 0.5, 201)];
        let off_a = pool.intern(&a);
        let off_b = pool.intern(&b);
        let off_a2 = pool.intern(&a);
        assert_eq!(off_a, 0);
        assert_eq!(off_a2, off_a, "identical sequence must reuse its template");
        assert_eq!(off_b, 2, "different duration must get a new template");
        assert_eq!(pool.stages().len(), 4);
        // 模板值逐位保持
        assert_eq!(pool.stages()[0].target_val, 1.0);
        assert_eq!(pool.stages()[1].duration, 200);
        assert_eq!(pool.stages()[3].duration, 201);
    }

    /// 淡出模板只驻留一次，且与普通模板互不干扰。
    #[test]
    fn fade_template_is_shared() {
        let mut pool = EnvTemplatePool::default();
        let normal = [stage(0, 1.0, 480)];
        let n = pool.intern(&normal);
        let f1 = pool.intern_fade(48_000);
        let f2 = pool.intern_fade(48_000);
        assert_eq!(f1, f2);
        assert_ne!(f1, n);
        assert_eq!(pool.stages()[f1 as usize].duration, 240);
        assert_eq!(pool.stages()[f1 as usize].target_val, 0.0);
    }

    /// 水位只覆盖尚未上传的尾部；标记后不再重复上传。
    #[test]
    fn pending_tracks_only_the_appended_tail() {
        let mut pool = EnvTemplatePool::default();
        pool.intern(&[stage(0, 1.0, 10)]);
        assert_eq!(pool.pending().len(), 1);
        pool.mark_uploaded();
        assert!(pool.pending().is_empty());
        pool.intern(&[stage(0, 1.0, 20)]);
        assert_eq!(pool.pending().len(), 1);
        assert_eq!(pool.pending()[0].duration, 20);
    }
}
