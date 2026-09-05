//! Public CPU boundary used by stock's collision-plane SIMD path.

use solarity_cpu::reciprocal_sqrt_estimate;

/// The instruction's estimate is retained without a software refinement step.
#[test]
fn reciprocal_sqrt_preserves_hardware_estimate_contract() {
    #[cfg(target_arch = "x86_64")]
    assert!(reciprocal_sqrt_estimate(1.0).is_some());
    for value in [0.0001_f32, 0.1, 1.0, 2.0, 3.0, 100.0, 1.0e10] {
        if let Some(estimate) = reciprocal_sqrt_estimate(value) {
            let exact = f64::from(value).sqrt().recip();
            assert!((f64::from(estimate) / exact - 1.0).abs() <= 1.5 / 4096.0);
        }
    }
    if let Some(zero) = reciprocal_sqrt_estimate(0.0) {
        assert_eq!(zero, f32::INFINITY);
        assert_eq!(reciprocal_sqrt_estimate(f32::INFINITY), Some(0.0));
        assert!(reciprocal_sqrt_estimate(-1.0).is_some_and(f32::is_nan));
    }
}
