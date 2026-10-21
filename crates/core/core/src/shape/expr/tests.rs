//! 公式求值测试

use std::collections::BTreeMap;

use super::*;

fn eval(src: &str, x: f64) -> f64 {
    compile(src, &[])
        .expect("公式应可编译")
        .eval(x, &BTreeMap::new())
        .expect("公式应可求值")
}

#[test]
fn test_arithmetic_and_precedence() {
    assert_eq!(eval("2 + 3 * 4", 0.0), 14.0);
    assert_eq!(eval("(2 + 3) * 4", 0.0), 20.0);
    assert_eq!(eval("2^3^2", 0.0), 512.0, "^ 右结合");
    assert_eq!(eval("-2^2", 0.0), -4.0, "一元负低于幂");
    assert_eq!(eval("2 * -3", 0.0), -6.0);
    assert_eq!(eval("7 % 4", 0.0), 3.0);
}

#[test]
fn test_constants_and_variables() {
    let mut vars = BTreeMap::new();
    vars.insert("height".to_string(), 4.0);
    let expr = compile("height * sin(x * 2 * pi)", &["height"]).expect("应编译");
    let value = expr.eval(0.25, &vars).expect("应求值");
    assert!((value - 4.0).abs() < 1e-9);
    assert!((eval("pi", 0.0) - std::f64::consts::PI).abs() < 1e-12);
}

#[test]
fn test_whitelisted_functions() {
    assert!((eval("sin(pi/2)", 0.0) - 1.0).abs() < 1e-12);
    assert!((eval("atan2(1,1)", 0.0) - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    assert_eq!(eval("min(3,1,2)", 0.0), 1.0);
    assert_eq!(eval("max(3,1,2)", 0.0), 3.0);
    assert_eq!(eval("clamp(5,0,3)", 0.0), 3.0);
    assert_eq!(eval("floor(1.9)", 0.0), 1.0);
    assert_eq!(eval("ceil(1.1)", 0.0), 2.0);
    assert_eq!(eval("round(1.6)", 0.0), 2.0);
    assert_eq!(eval("sign(-9)", 0.0), -1.0);
    assert!((eval("ln(e)", 0.0) - 1.0).abs() < 1e-12);
    assert!((eval("log10(1000)", 0.0) - 3.0).abs() < 1e-12);
    assert_eq!(eval("pow(2,10)", 0.0), 1024.0);
}

#[test]
fn test_invalid_inputs_return_err() {
    for src in [
        "", "1/0", "sin(", "foo(1)", "y + 1", "sin(1,2)", "2^65", "sqrt(-1)",
    ] {
        assert!(
            compile(src, &[]).is_err()
                || compile(src, &[])
                    .expect("可编译")
                    .eval(0.0, &BTreeMap::new())
                    .is_err(),
            "应拒绝：{src}"
        );
    }
}

#[test]
fn test_limits_and_missing_values() {
    let long = "1+".repeat(2100) + "1";
    assert!(compile(&long, &[]).is_err(), "超长应拒绝");
    let deep = format!("{}1{}", "(".repeat(65), ")".repeat(65));
    assert!(compile(&deep, &[]).is_err(), "过深应拒绝");
    let expr = compile("height + 1", &["height"]).expect("应编译");
    assert!(expr.eval(0.0, &BTreeMap::new()).is_err(), "缺变量应报错");
}

#[test]
fn test_division_by_zero_at_eval() {
    let expr = compile("1 / (x - x)", &[]).expect("应编译");
    assert_eq!(
        expr.eval(0.0, &BTreeMap::new()),
        Err(ExprError::DivisionByZero)
    );
}
