//! Monotone cubic Hermite spline used to map noise parameters to heights.

use super::config::SplineConfig;

/// Piecewise cubic interpolation between sorted control points.
///
/// Uses the Fritsch-Carlson tangent limiter, so the curve does not overshoot its control
/// points. Inputs outside the control range clamp to the first or last point.
pub(crate) struct Spline {
    points: Vec<(f32, f32)>,
    tangents: Vec<f32>,
}

impl Spline {
    pub(crate) fn new(config: &SplineConfig) -> Self {
        let points = config.points.clone();
        let tangents = tangents(&points);

        Self { points, tangents }
    }

    pub(crate) fn sample(&self, t: f32) -> f32 {
        let first = self.points[0];
        let last = *self.points.last().expect("spline has at least two points");

        if t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }

        let index = self
            .points
            .windows(2)
            .position(|window| t < window[1].0)
            .unwrap_or(self.points.len() - 2);

        let (x0, y0) = self.points[index];
        let (x1, y1) = self.points[index + 1];
        let width = x1 - x0;
        let s = (t - x0) / width;

        let s2 = s * s;
        let s3 = s2 * s;

        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;

        h00 * y0
            + h10 * width * self.tangents[index]
            + h01 * y1
            + h11 * width * self.tangents[index + 1]
    }
}

fn tangents(points: &[(f32, f32)]) -> Vec<f32> {
    let count = points.len();
    let mut secants = vec![0.0; count - 1];

    for (index, window) in points.windows(2).enumerate() {
        secants[index] = (window[1].1 - window[0].1) / (window[1].0 - window[0].0);
    }

    let mut tangents = vec![0.0; count];
    tangents[0] = secants[0];
    tangents[count - 1] = secants[count - 2];

    for index in 1..count - 1 {
        tangents[index] = if secants[index - 1] * secants[index] <= 0.0 {
            0.0
        } else {
            (secants[index - 1] + secants[index]) / 2.0
        };
    }

    for index in 0..count - 1 {
        if secants[index] == 0.0 {
            tangents[index] = 0.0;
            tangents[index + 1] = 0.0;
            continue;
        }

        let left = tangents[index] / secants[index];
        let right = tangents[index + 1] / secants[index];
        let norm = left.hypot(right);

        if norm > 3.0 {
            let scale = 3.0 / norm;
            tangents[index] = scale * left * secants[index];
            tangents[index + 1] = scale * right * secants[index];
        }
    }

    tangents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spline(points: Vec<(f32, f32)>) -> Spline {
        Spline::new(&SplineConfig { points })
    }

    #[test]
    fn two_points_are_linear() {
        let spline = spline(vec![(-1.0, 10.0), (1.0, 30.0)]);

        assert!((spline.sample(-1.0) - 10.0).abs() < f32::EPSILON);
        assert!((spline.sample(0.0) - 20.0).abs() < f32::EPSILON);
        assert!((spline.sample(1.0) - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn passes_through_control_points() {
        let spline = spline(vec![(-1.0, 70.0), (0.0, 100.0), (1.0, 160.0)]);

        assert!((spline.sample(-1.0) - 70.0).abs() < 0.001);
        assert!((spline.sample(0.0) - 100.0).abs() < 0.001);
        assert!((spline.sample(1.0) - 160.0).abs() < 0.001);
    }

    #[test]
    fn clamps_outside_the_control_range() {
        let spline = spline(vec![(-0.5, 10.0), (0.5, 20.0)]);

        assert!((spline.sample(-10.0) - 10.0).abs() < f32::EPSILON);
        assert!((spline.sample(10.0) - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn does_not_overshoot_between_monotone_points() {
        let spline = spline(vec![(-1.0, 0.0), (0.0, 100.0), (1.0, 101.0)]);

        for step in 0..=100 {
            let t = -1.0 + step as f32 / 50.0;
            assert!((-0.001..=101.001).contains(&spline.sample(t)));
        }
    }
}
