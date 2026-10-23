//! 形状力度包络（DRAW-028）
//!
//! 包络存的是**形状自身时间跨度上的比例** `u`（0 = 最早点，1 = 最晚点），
//! 因此形状移动/拉伸时力度自动跟随。同一 `u` 上两点表示跳变。

use serde::{Deserialize, Serialize};

/// 包络比较容差
pub const ENV_EPS: f32 = 1e-9;

/// 形状力度包络
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VelocityEnv {
    /// `[[u, velocity], ...]`
    pub points: Vec<[f32; 2]>,
}

impl VelocityEnv {
    /// 空包络 = 默认力度 100
    pub fn new(points: Vec<[f32; 2]>) -> Self {
        let mut env = Self { points };
        tidy_env(&mut env);
        env
    }
}

/// 取 `u` 处力度；正好落在跳变上时默认取右侧，`left = true` 取左侧
pub fn env_at(env: &VelocityEnv, u: f32, left: bool) -> u8 {
    if env.points.is_empty() {
        return 100;
    }
    let mut points = env.points.clone();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let u = u.clamp(0.0, 1.0);
    let hits = points
        .iter()
        .filter(|point| (point[0] - u).abs() <= ENV_EPS)
        .collect::<Vec<_>>();
    if !hits.is_empty() {
        let value = if left {
            hits[0][1]
        } else {
            hits[hits.len() - 1][1]
        };
        return clamp_velocity(value);
    }
    if u <= points[0][0] {
        return clamp_velocity(points[0][1]);
    }
    if u >= points[points.len() - 1][0] {
        return clamp_velocity(points[points.len() - 1][1]);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a[0]..b[0]).contains(&u) {
            let span = b[0] - a[0];
            let t = if span.abs() <= ENV_EPS {
                1.0
            } else {
                (u - a[0]) / span
            };
            return clamp_velocity(a[1] + (b[1] - a[1]) * t);
        }
    }
    100
}

/// 排序、夹取 u/力度；保留同 u 的跳变
pub fn tidy_env(env: &mut VelocityEnv) {
    env.points
        .retain(|point| point[0].is_finite() && point[1].is_finite());
    for point in &mut env.points {
        point[0] = point[0].clamp(0.0, 1.0);
        point[1] = f32::from(clamp_velocity(point[1]));
    }
    env.points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    env.points
        .dedup_by(|a, b| (a[0] - b[0]).abs() <= ENV_EPS && (a[1] - b[1]).abs() <= ENV_EPS);
}

/// 用同一力度涂一段；区间内的旧点被替换，边界保留
pub fn paint_env(env: &mut VelocityEnv, from: f32, to: f32, velocity: u8) {
    let mut lo = from.clamp(0.0, 1.0);
    let mut hi = to.clamp(0.0, 1.0);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    let value = f32::from(velocity.clamp(1, 127));
    env.points.retain(|point| point[0] < lo || point[0] > hi);
    env.points.push([lo, value]);
    env.points.push([hi, value]);
    tidy_env(env);
}

fn clamp_velocity(value: f32) -> u8 {
    value.round().clamp(1.0, 127.0) as u8
}

#[cfg(test)]
mod tests;
