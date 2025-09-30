pub fn bounce_easing(t: f64) -> f64 {
    let n1 = 7.5625;
    let d1 = 2.75;

    if t < (1.0 / d1) {
        n1 * t * t
    } else if t < (2.0 / d1) {
        n1 * (t - 1.5 / d1) * (t - 1.5 / d1) + 0.75
    } else if t < (2.5 / d1) {
        n1 * (t - 2.25 / d1) * (t - 2.25 / d1) + 0.9375
    } else {
        n1 * (t - 2.625 / d1) * (t - 2.625 / d1) + 0.984375
    }
}
