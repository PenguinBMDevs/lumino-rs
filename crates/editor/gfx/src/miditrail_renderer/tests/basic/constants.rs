use super::*;

#[test]
fn test_cube_constants() {
    assert_eq!(MiditrailRenderer::CUBE_VERTICES.len(), 144);
    assert_eq!(MiditrailRenderer::CUBE_INDICES.len(), 36);
}

/// 平面索引语义（盒子→平面的唯一改动点，纯 CPU 验证）：
/// - 仅顶面 6（y=1 的 X-Z 面）= 立方体索引 [0..6) 逐字复用；
/// - Y 即高度轴（scale[1]=NOTE_HEIGHT），平面只压 Y，X/Z 保留；
/// - 角点 y 全为 1、x/z 取遍 {0,1}（Z 长不丢失），索引不越界。
#[test]
fn test_quad_indices_reuse_cube_faces() {
    assert_eq!(MiditrailRenderer::QUAD_INDICES.len(), 6);
    assert_eq!(MiditrailRenderer::QUAD_RANGE, 0..6);
    let cube = MiditrailRenderer::CUBE_INDICES;
    let quad = MiditrailRenderer::QUAD_INDICES;
    assert_eq!(&quad[..], &cube[0..6], "平面必须逐字复用顶面");
    // 角点坐标约束：位置 stride 6（pos3 + normal3），24 个顶点。
    let verts = MiditrailRenderer::CUBE_VERTICES;
    let (mut xs, mut zs) = (vec![], vec![]);
    for &i in quad.iter() {
        let base = (i as usize) * 6;
        assert!(base + 2 < verts.len(), "平面索引越界: {i}");
        assert_eq!(verts[base + 1], 1.0, "顶面角点 y 必须为 1: {i}");
        xs.push(verts[base]);
        zs.push(verts[base + 2]);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).expect("角点 x 非法"));
    zs.sort_by(|a, b| a.partial_cmp(b).expect("角点 z 非法"));
    assert_eq!((xs[0], xs[3]), (0.0, 1.0), "X 宽必须保留");
    assert_eq!((zs[0], zs[3]), (0.0, 1.0), "Z 长必须保留");
}
