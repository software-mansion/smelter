use std::f64::consts::PI;

const ALLOWED_FLOATING_ERROR: f64 = 1e-7;

pub fn cubic_bezier_easing(progress: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if progress.is_close_to(0.0) {
        return 0.0;
    }
    if progress.is_close_to(1.0) {
        return 1.0;
    }

    let t = find_first_cubic_root(-progress, x1 - progress, x2 - progress, 1.0 - progress);
    if t.is_nan() {
        // TODO(noituri): IDK how to handle this case
        return 1.0;
    }

    cubic_bezier(t, y1, y2).clamp(0.0, 1.0)
}

fn cubic_bezier(t: f64, p1: f64, p2: f64) -> f64 {
    let a = 1.0 / 3.0 + (p1 - p2);
    let b = p2 - 2.0 * p1;
    let c = p1;

    3.0 * ((a * t + b) * t + c) * t
}

// Based on https://pomax.github.io/bezierinfo/#yforx
// https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Bezier.kt;l=274;drc=9b09524f2b2cbfc397269fbbe1267dd46b14077a
fn find_first_cubic_root(p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    // Coefficients calculated from control points
    let a = 3.0 * (p0 - 2.0 * p1 + p2);
    let b = 3.0 * (p1 - p0);
    let c = p0;
    let d = -p0 + 3.0 * (p1 - p2) + p3;

    // If `d` is 0 then the function is not cubic
    if d.is_close_to(0.0) {
        // If `a` is 0 then the function is not quadratic
        if a.is_close_to(0.0) {
            // If `b` is 0 then the function is not linear
            if b.is_close_to(0.0) {
                return f64::NAN;
            }

            return (-c / b).clamp_valid_root_in_unit_range();
        }
        let q = f64::sqrt(b * b - 4.0 * a * c);
        let a2 = 2.0 * a;
        let root = ((q - b) / a2).clamp_valid_root_in_unit_range();
        if !root.is_nan() {
            return root;
        }

        return ((-b - q) / a2).clamp_valid_root_in_unit_range();
    }

    let a = a / d;
    let b = b / d;
    let c = c / d;

    let o3 = (3.0 * b - a.powi(2)) / 9.0;
    let q2 = (2.0 * a.powi(3) - 9.0 * a * b + 27.0 * c) / 54.0;
    let a3 = a / 3.0;
    let discriminant = q2.powi(2) + o3.powi(3);

    if discriminant < 0.0 {
        let mp33 = -f64::powi(o3, 3);
        let r = mp33.sqrt();
        let cos_phi = (-q2 / r).clamp(-1.0, 1.0);
        let phi = cos_phi.acos();
        let t1 = 2.0 * r.cbrt();

        let root = (t1 * f64::cos(phi / 3.0) - a3).clamp_valid_root_in_unit_range();
        if !root.is_nan() {
            return root;
        }

        let root = (t1 * f64::cos((phi + 2.0 * PI) / 3.0) - a3).clamp_valid_root_in_unit_range();
        if !root.is_nan() {
            return root;
        }

        return (t1 * f64::cos((phi + 4.0 * PI) / 3.0) - a3).clamp_valid_root_in_unit_range();
    }

    if discriminant == 0.0 {
        let u1 = -f64::cbrt(q2);
        let root = (2.0 * u1 - a3).clamp_valid_root_in_unit_range();
        if !root.is_nan() {
            return root;
        }

        return (-u1 - a3).clamp_valid_root_in_unit_range();
    }

    let sd = discriminant.sqrt();
    let u1 = (-q2 + sd).cbrt();
    let v1 = (q2 + sd).cbrt();

    (u1 - v1 - a3).clamp_valid_root_in_unit_range()
}

trait F64Ext {
    fn is_close_to(&self, value: Self) -> bool;

    fn clamp_valid_root_in_unit_range(self) -> Self;
}

impl F64Ext for f64 {
    fn is_close_to(&self, value: Self) -> bool {
        (*self - value).abs() < ALLOWED_FLOATING_ERROR
    }

    fn clamp_valid_root_in_unit_range(self) -> Self {
        if self < 0.0 {
            if self >= -ALLOWED_FLOATING_ERROR {
                0.0
            } else {
                f64::NAN
            }
        } else if self > 1.0 {
            if self <= 1.0 + ALLOWED_FLOATING_ERROR {
                1.0
            } else {
                f64::NAN
            }
        } else {
            self
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cubic_bezier_easing() {
        assert!(cubic_bezier_easing(0.0, 0.0, 0.0, 1.0, 1.0).is_close_to(0.0));
        assert!(cubic_bezier_easing(1.0, 0.0, 0.0, 1.0, 1.0).is_close_to(1.0));
        assert!(cubic_bezier_easing(0.5, 0.0, 0.0, 1.0, 1.0).is_close_to(0.5));
        assert!(cubic_bezier_easing(0.294, 0.25, 0.1, 0.25, 1.0).is_close_to(0.5014012915764126));
        assert!(cubic_bezier_easing(0.5, 0.85, 0.0, 0.15, 1.0).is_close_to(0.5));
    }
}
