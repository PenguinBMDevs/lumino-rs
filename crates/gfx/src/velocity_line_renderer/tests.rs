use super::*;

#[test]
fn test_line_instance_creation() {
    let inst = VelocityLineInstance {
        start: [10.0, 20.0],
        end: [100.0, 80.0],
        color: [0.3, 0.7, 0.9, 1.0],
    };
    assert_eq!(inst.start, [10.0, 20.0]);
    assert_eq!(inst.end, [100.0, 80.0]);
    assert_eq!(inst.color, [0.3, 0.7, 0.9, 1.0]);
}

#[test]
fn test_circle_instance_creation() {
    let inst = VelocityCircleInstance {
        center: [50.0, 50.0],
        radius: 4.0,
        _pad: 0.0,
        color: [0.3, 0.7, 0.9, 1.0],
    };
    assert_eq!(inst.center, [50.0, 50.0]);
    assert_eq!(inst.radius, 4.0);
    assert_eq!(inst.color, [0.3, 0.7, 0.9, 1.0]);
}

#[test]
fn test_velocity_instance_layout_sizes() {
    assert_eq!(std::mem::size_of::<VelocityLineInstance>(), 32);
    assert_eq!(std::mem::size_of::<VelocityCircleInstance>(), 32);
}
