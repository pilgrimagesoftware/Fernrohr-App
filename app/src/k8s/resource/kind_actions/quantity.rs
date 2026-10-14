//! A Kubernetes resource quantity (`10Gi`, `500M`, `1.5Ti`, `2e9`) in bytes,
//! so Expand can refuse a size no larger than the claim's - Kubernetes never
//! shrinks a PersistentVolumeClaim.

/// `quantity` in bytes, or `None` if it isn't one.
pub(crate) fn bytes(quantity: &str) -> Option<f64> {
    let quantity = quantity.trim();
    const SUFFIXES: [(&str, f64); 13] = [
        ("Ki", 1024.0),
        ("Mi", 1_048_576.0),
        ("Gi", 1_073_741_824.0),
        ("Ti", 1_099_511_627_776.0),
        ("Pi", 1_125_899_906_842_624.0),
        ("Ei", 1_152_921_504_606_846_976.0),
        ("m", 0.001),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
    ];
    let (number, scale) = SUFFIXES
        .iter()
        .find_map(|(suffix, scale)| quantity.strip_suffix(suffix).map(|number| (number, *scale)))
        .unwrap_or((quantity, 1.0));
    let plain = |c: char| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E');
    if number.is_empty() || !number.chars().all(plain) {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value * scale)
}
