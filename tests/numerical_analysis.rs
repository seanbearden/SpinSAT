//! Numerical analysis sanity checks for integration methods.
//!
//! Tests Euler, Trapezoid, and RK4 against standard ODE problems with known
//! exact solutions. These tests are independent of the DMM/SAT solver — they
//! verify the numerical integration math is correct.
//!
//! If any of these tests fail after a refactor, the integrator math is broken.

/// Generic ODE integrator: given dy/dt = f(t, y), advance one step.
/// Returns new (t, y) after stepping by dt.
type StepFn = fn(f: &dyn Fn(f64, &[f64]) -> Vec<f64>, t: f64, y: &[f64], dt: f64) -> Vec<f64>;

/// Forward Euler: y_{n+1} = y_n + dt * f(t_n, y_n)
fn euler_step(f: &dyn Fn(f64, &[f64]) -> Vec<f64>, t: f64, y: &[f64], dt: f64) -> Vec<f64> {
    let dy = f(t, y);
    y.iter()
        .zip(dy.iter())
        .map(|(yi, dyi)| yi + dt * dyi)
        .collect()
}

/// Trapezoid (Heun): y_{n+1} = y_n + dt/2 * (f(t_n, y_n) + f(t_{n+1}, y_pred))
fn trapezoid_step(f: &dyn Fn(f64, &[f64]) -> Vec<f64>, t: f64, y: &[f64], dt: f64) -> Vec<f64> {
    let k1 = f(t, y);
    let y_pred: Vec<f64> = y
        .iter()
        .zip(k1.iter())
        .map(|(yi, ki)| yi + dt * ki)
        .collect();
    let k2 = f(t + dt, &y_pred);
    y.iter()
        .zip(k1.iter().zip(k2.iter()))
        .map(|(yi, (k1i, k2i))| yi + dt * 0.5 * (k1i + k2i))
        .collect()
}

/// Classical RK4
fn rk4_step(f: &dyn Fn(f64, &[f64]) -> Vec<f64>, t: f64, y: &[f64], dt: f64) -> Vec<f64> {
    let n = y.len();
    let k1 = f(t, y);

    let y2: Vec<f64> = (0..n).map(|i| y[i] + 0.5 * dt * k1[i]).collect();
    let k2 = f(t + 0.5 * dt, &y2);

    let y3: Vec<f64> = (0..n).map(|i| y[i] + 0.5 * dt * k2[i]).collect();
    let k3 = f(t + 0.5 * dt, &y3);

    let y4: Vec<f64> = (0..n).map(|i| y[i] + dt * k3[i]).collect();
    let k4 = f(t + dt, &y4);

    (0..n)
        .map(|i| y[i] + dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]))
        .collect()
}

/// Integrate an ODE from t=0 to t=t_final using n_steps of the given method.
fn integrate(
    step_fn: StepFn,
    f: &dyn Fn(f64, &[f64]) -> Vec<f64>,
    y0: &[f64],
    t_final: f64,
    n_steps: usize,
) -> Vec<f64> {
    let dt = t_final / n_steps as f64;
    let mut y = y0.to_vec();
    let mut t = 0.0;
    for _ in 0..n_steps {
        y = step_fn(f, t, &y, dt);
        t += dt;
    }
    y
}

// ============================================================================
// Test Problem 1: Exponential Decay
// ẏ = -y, y(0) = 1 → y(t) = e^(-t)
// ============================================================================

fn exp_decay(_t: f64, y: &[f64]) -> Vec<f64> {
    vec![-y[0]]
}

#[test]
fn test_euler_exp_decay() {
    let y = integrate(euler_step, &exp_decay, &[1.0], 1.0, 1000);
    let exact = (-1.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-3, "Euler exp decay error too large: {err}");
}

#[test]
fn test_trapezoid_exp_decay() {
    let y = integrate(trapezoid_step, &exp_decay, &[1.0], 1.0, 100);
    let exact = (-1.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-4, "Trapezoid exp decay error too large: {err}");
}

#[test]
fn test_rk4_exp_decay() {
    let y = integrate(rk4_step, &exp_decay, &[1.0], 1.0, 10);
    let exact = (-1.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-6, "RK4 exp decay error too large: {err}");
}

// ============================================================================
// Test Problem 2: Exponential Growth
// ẏ = y, y(0) = 1 → y(t) = e^t
// ============================================================================

fn exp_growth(_t: f64, y: &[f64]) -> Vec<f64> {
    vec![y[0]]
}

#[test]
fn test_euler_exp_growth() {
    let y = integrate(euler_step, &exp_growth, &[1.0], 1.0, 1000);
    let exact = 1.0_f64.exp();
    let err = (y[0] - exact).abs();
    assert!(err < 5e-3, "Euler exp growth error too large: {err}");
}

#[test]
fn test_trapezoid_exp_growth() {
    let y = integrate(trapezoid_step, &exp_growth, &[1.0], 1.0, 100);
    let exact = 1.0_f64.exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-4, "Trapezoid exp growth error too large: {err}");
}

#[test]
fn test_rk4_exp_growth() {
    let y = integrate(rk4_step, &exp_growth, &[1.0], 1.0, 10);
    let exact = 1.0_f64.exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-5, "RK4 exp growth error too large: {err}");
}

// ============================================================================
// Test Problem 3: Stiff Decay
// ẏ = -50y, y(0) = 1 → y(t) = e^(-50t)
// Euler needs small dt for stability (dt < 2/50 = 0.04)
// ============================================================================

fn stiff_decay(_t: f64, y: &[f64]) -> Vec<f64> {
    vec![-50.0 * y[0]]
}

#[test]
fn test_euler_stiff_decay() {
    // Euler needs dt < 0.04 for stability; use 5000 steps for dt=0.0002
    let y = integrate(euler_step, &stiff_decay, &[1.0], 1.0, 5000);
    let exact = (-50.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-2, "Euler stiff decay error too large: {err}");
    // Also verify it didn't blow up (stability check)
    assert!(
        y[0].abs() < 1.0,
        "Euler stiff: solution should have decayed, got {}",
        y[0]
    );
}

#[test]
fn test_trapezoid_stiff_decay() {
    // Trapezoid is A-stable, should handle stiffness with fewer steps
    let y = integrate(trapezoid_step, &stiff_decay, &[1.0], 1.0, 500);
    let exact = (-50.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-3, "Trapezoid stiff decay error too large: {err}");
}

#[test]
fn test_rk4_stiff_decay() {
    let y = integrate(rk4_step, &stiff_decay, &[1.0], 1.0, 100);
    let exact = (-50.0_f64).exp();
    let err = (y[0] - exact).abs();
    assert!(err < 1e-4, "RK4 stiff decay error too large: {err}");
}

// ============================================================================
// Test Problem 4: Harmonic Oscillator
// ẍ = -x → system: ẋ = v, v̇ = -x
// x(0) = 1, v(0) = 0 → x(t) = cos(t), v(t) = -sin(t)
// ============================================================================

fn harmonic(_t: f64, y: &[f64]) -> Vec<f64> {
    // y[0] = x, y[1] = v
    vec![y[1], -y[0]]
}

#[test]
fn test_euler_harmonic() {
    let y = integrate(euler_step, &harmonic, &[1.0, 0.0], 1.0, 10000);
    let exact_x = 1.0_f64.cos();
    let exact_v = -(1.0_f64.sin());
    let err_x = (y[0] - exact_x).abs();
    let err_v = (y[1] - exact_v).abs();
    assert!(err_x < 1e-3, "Euler harmonic x error: {err_x}");
    assert!(err_v < 1e-3, "Euler harmonic v error: {err_v}");
}

#[test]
fn test_trapezoid_harmonic() {
    let y = integrate(trapezoid_step, &harmonic, &[1.0, 0.0], 1.0, 100);
    let exact_x = 1.0_f64.cos();
    let exact_v = -(1.0_f64.sin());
    let err_x = (y[0] - exact_x).abs();
    let err_v = (y[1] - exact_v).abs();
    assert!(err_x < 1e-4, "Trapezoid harmonic x error: {err_x}");
    assert!(err_v < 1e-4, "Trapezoid harmonic v error: {err_v}");
}

#[test]
fn test_rk4_harmonic() {
    let y = integrate(rk4_step, &harmonic, &[1.0, 0.0], 1.0, 10);
    let exact_x = 1.0_f64.cos();
    let exact_v = -(1.0_f64.sin());
    let err_x = (y[0] - exact_x).abs();
    let err_v = (y[1] - exact_v).abs();
    assert!(err_x < 1e-5, "RK4 harmonic x error: {err_x}");
    assert!(err_v < 1e-5, "RK4 harmonic v error: {err_v}");
}

// ============================================================================
// Test Problem 5: Polynomial (RK4 exactness)
// ẏ = t², y(0) = 0 → y(t) = t³/3
// RK4 is exact for polynomials up to degree 4, so this should be exact
// to machine precision regardless of step count.
// ============================================================================

fn polynomial(t: f64, _y: &[f64]) -> Vec<f64> {
    vec![t * t]
}

#[test]
fn test_rk4_polynomial_exact() {
    // Even with just 2 steps, RK4 should be exact for cubic
    let y = integrate(rk4_step, &polynomial, &[0.0], 1.0, 2);
    let exact = 1.0 / 3.0;
    let err = (y[0] - exact).abs();
    assert!(err < 1e-14, "RK4 polynomial should be exact, error: {err}");
}

#[test]
fn test_rk4_polynomial_exact_single_step() {
    // Even a single step should be exact
    let y = integrate(rk4_step, &polynomial, &[0.0], 1.0, 1);
    let exact = 1.0 / 3.0;
    let err = (y[0] - exact).abs();
    assert!(
        err < 1e-14,
        "RK4 single step polynomial should be exact, error: {err}"
    );
}

// ============================================================================
// Convergence Order Tests
// Verify that halving h reduces error by the expected factor:
//   Euler: ~2x (order 1)
//   Trapezoid: ~4x (order 2)
//   RK4: ~16x (order 4)
// ============================================================================

fn measure_error(step_fn: StepFn, n_steps: usize) -> f64 {
    let y = integrate(step_fn, &exp_decay, &[1.0], 1.0, n_steps);
    let exact = (-1.0_f64).exp();
    (y[0] - exact).abs()
}

#[test]
fn test_euler_convergence_order_1() {
    let e1 = measure_error(euler_step, 100);
    let e2 = measure_error(euler_step, 200);
    let e3 = measure_error(euler_step, 400);

    let ratio1 = e1 / e2;
    let ratio2 = e2 / e3;

    // Order 1: error halves when h halves → ratio ≈ 2
    assert!(
        ratio1 > 1.5 && ratio1 < 2.5,
        "Euler convergence ratio1={ratio1}, expected ~2"
    );
    assert!(
        ratio2 > 1.5 && ratio2 < 2.5,
        "Euler convergence ratio2={ratio2}, expected ~2"
    );
}

#[test]
fn test_trapezoid_convergence_order_2() {
    let e1 = measure_error(trapezoid_step, 50);
    let e2 = measure_error(trapezoid_step, 100);
    let e3 = measure_error(trapezoid_step, 200);

    let ratio1 = e1 / e2;
    let ratio2 = e2 / e3;

    // Order 2: error quarters when h halves → ratio ≈ 4
    assert!(
        ratio1 > 3.0 && ratio1 < 5.0,
        "Trapezoid convergence ratio1={ratio1}, expected ~4"
    );
    assert!(
        ratio2 > 3.0 && ratio2 < 5.0,
        "Trapezoid convergence ratio2={ratio2}, expected ~4"
    );
}

#[test]
fn test_rk4_convergence_order_4() {
    let e1 = measure_error(rk4_step, 10);
    let e2 = measure_error(rk4_step, 20);
    let e3 = measure_error(rk4_step, 40);

    let ratio1 = e1 / e2;
    let ratio2 = e2 / e3;

    // Order 4: error reduces by 16x when h halves → ratio ≈ 16
    assert!(
        ratio1 > 12.0 && ratio1 < 20.0,
        "RK4 convergence ratio1={ratio1}, expected ~16"
    );
    assert!(
        ratio2 > 12.0 && ratio2 < 20.0,
        "RK4 convergence ratio2={ratio2}, expected ~16"
    );
}
