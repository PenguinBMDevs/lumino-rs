//! Miditrail 3D 渲染器数学工具
//!
//! 列主序 4x4 矩阵与向量工具，避免引入额外依赖。

use super::types::MiditrailCameraGpu;

/// 构建相机 Uniform（投影 * 视图）。
pub fn build_camera_uniform(width: u32, height: u32) -> MiditrailCameraGpu {
    let aspect = if height == 0 {
        1.0
    } else {
        width as f32 / height as f32
    };
    let eye = [0.5f32, 0.466, 0.385];
    let center = [0.5f32, 0.05, -0.5];
    let up = [0.0f32, 1.0, 0.0];
    let fov_deg = 59.327f32;
    let fov = fov_deg.to_radians();
    let near = 0.01f32;
    let far = 100.0f32;
    let view = look_at_rh(eye, center, up);
    let proj = perspective_rh(fov, aspect, near, far);
    let view_proj = mat4_mul(proj, view);
    let light_dir = normalize([0.3, 0.8, -0.5]);
    MiditrailCameraGpu {
        view_proj,
        light_dir,
        ambient: 0.3,
    }
}

pub fn look_at_rh(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);

    // WGSL 矩阵为列主序，外层数组下标为列
    [
        [s[0], s[1], s[2], -dot(s, eye)],
        [u[0], u[1], u[2], -dot(u, eye)],
        [-f[0], -f[1], -f[2], dot(f, eye)],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, 2.0 * far * near * nf, 0.0],
    ]
}

pub fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0f32;
            for k in 0..4 {
                sum += a[k][row] * b[col][k];
            }
            r[col][row] = sum;
        }
    }
    r
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 {
        return [0.0, 1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_utilities() {
        let eye = [0.5f32, 0.5, 0.5];
        let center = [0.5f32, 0.0, -0.5];
        let up = [0.0f32, 1.0, 0.0];
        let view = look_at_rh(eye, center, up);
        let proj = perspective_rh(0.785398, 16.0 / 9.0, 0.01, 100.0);
        let _vp = mat4_mul(proj, view);
        assert!((dot([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_camera_uniform_size() {
        let cam = build_camera_uniform(1920, 1080);
        assert_eq!(cam.light_dir[0].is_finite(), true);
    }
}
