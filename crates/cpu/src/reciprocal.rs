//! The SSE estimate used by stock collision-plane preparation.

/// Returns the hardware `RSQRTSS` estimate when SSE is available.
///
/// This deliberately does not refine the estimate: build 12340 uses it directly
/// when its CPU feature test enables the SIMD collision path. Other platforms
/// return `None`, allowing the caller to use the stock scalar path.
#[must_use]
#[allow(unsafe_code)]
pub fn reciprocal_sqrt_estimate(value: f32) -> Option<f32> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        #[cfg(target_arch = "x86")]
        use std::arch::x86::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
        if std::is_x86_feature_detected!("sse") {
            // SAFETY: SSE was detected above; these register-only intrinsics
            // accept every float bit pattern and do not access memory.
            return Some(unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(value))) });
        }
    }
    let _ = value;
    None
}
