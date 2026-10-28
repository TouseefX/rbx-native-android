//! Evaluates `NumberSequence`/`ColorSequence` at a normalized time, the way
//! `ParticleEmitter.Size`/`Transparency`/`Color` are read over a particle's age.
//!
//! A keypoint's `envelope` is the random spread around its value a particle
//! draws from, see [`eval_number_spread`]; only particles use it, a beam's or
//! trail's sequence is read at its plain values.

use rbx_dom::{ColorSequence, NumberSequence, NumberSequenceKeypoint};

use crate::scene::srgb_to_linear;

/// Linearly interpolates `seq` at `t` (clamped to `[0, 1]`), the same rule
/// Roblox uses for a particle's age over its lifetime.
pub(crate) fn eval_number(seq: &NumberSequence, t: f32) -> f32 {
    eval_keypoints(seq, t, |keypoint| keypoint.value)
}

/// [`eval_number`] with every keypoint moved `spread` (in `[-1, 1]`) of its
/// envelope off its value: the docs' "chosen randomly within [value -
/// envelope, value + envelope]".
///
/// One `spread` per particle, for its whole life and every keypoint: measured
/// in Studio, a particle of a constant `Size` with an envelope keeps the size
/// it was born with rather than drifting between two random keypoint values.
pub(crate) fn eval_number_spread(seq: &NumberSequence, t: f32, spread: f32) -> f32 {
    eval_keypoints(seq, t, |keypoint| {
        keypoint.value + spread * keypoint.envelope
    })
}

fn eval_keypoints(
    seq: &NumberSequence,
    t: f32,
    value: impl Fn(&NumberSequenceKeypoint) -> f32,
) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let keypoints = &seq.keypoints;
    let Some(first) = keypoints.first() else {
        return 0.0;
    };
    if keypoints.len() == 1 || t <= first.time {
        return value(first);
    }
    for pair in keypoints.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t <= b.time {
            let span = b.time - a.time;
            let f = if span > 0.0 { (t - a.time) / span } else { 0.0 };
            return value(a) + (value(b) - value(a)) * f;
        }
    }
    keypoints.last().map_or(0.0, value)
}

/// Same rule as [`eval_number`], but for a colour gradient.
///
/// Interpolated in the sequence's own (sRGB) space, like Roblox's own
/// `ColorSequence`, then linearized once at the end, see [`srgb_to_linear`],
/// rather than linearizing each keypoint first, which would shift the gradient.
pub(crate) fn eval_color(seq: &ColorSequence, t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let keypoints = &seq.keypoints;
    let Some(first) = keypoints.first() else {
        return [0.0; 3];
    };
    let raw = |c: rbx_dom::Color3Data| [c.r, c.g, c.b];
    let raw = if keypoints.len() == 1 || t <= first.time {
        raw(first.color)
    } else {
        keypoints
            .windows(2)
            .find_map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                if t > b.time {
                    return None;
                }
                let span = b.time - a.time;
                let f = if span > 0.0 { (t - a.time) / span } else { 0.0 };
                let (ca, cb) = (raw(a.color), raw(b.color));
                Some([
                    ca[0] + (cb[0] - ca[0]) * f,
                    ca[1] + (cb[1] - ca[1]) * f,
                    ca[2] + (cb[2] - ca[2]) * f,
                ])
            })
            .unwrap_or_else(|| raw(keypoints.last().expect("checked non-empty above").color))
    };
    [
        srgb_to_linear(raw[0]),
        srgb_to_linear(raw[1]),
        srgb_to_linear(raw[2]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom::{Color3Data, ColorSequenceKeypoint};

    fn numbers(points: &[(f32, f32)]) -> NumberSequence {
        enveloped(&points.iter().map(|&(t, v)| (t, v, 0.0)).collect::<Vec<_>>())
    }

    fn enveloped(points: &[(f32, f32, f32)]) -> NumberSequence {
        NumberSequence {
            keypoints: points
                .iter()
                .map(|&(time, value, envelope)| NumberSequenceKeypoint {
                    time,
                    value,
                    envelope,
                })
                .collect(),
        }
    }

    fn colors(points: &[(f32, [f32; 3])]) -> ColorSequence {
        ColorSequence {
            keypoints: points
                .iter()
                .map(|&(time, [r, g, b])| ColorSequenceKeypoint {
                    time,
                    color: Color3Data { r, g, b },
                    envelope: 0.0,
                })
                .collect(),
        }
    }

    #[test]
    fn a_two_keypoint_sequence_interpolates_linearly_between_them() {
        let seq = numbers(&[(0.0, 0.0), (1.0, 10.0)]);
        assert_eq!(eval_number(&seq, 0.0), 0.0);
        assert_eq!(eval_number(&seq, 0.5), 5.0);
        assert_eq!(eval_number(&seq, 1.0), 10.0);
    }

    #[test]
    fn time_outside_zero_one_clamps_to_the_nearest_keypoint() {
        let seq = numbers(&[(0.0, 2.0), (1.0, 8.0)]);
        assert_eq!(eval_number(&seq, -1.0), 2.0);
        assert_eq!(eval_number(&seq, 2.0), 8.0);
    }

    #[test]
    fn a_middle_keypoint_is_reached_exactly_at_its_own_time() {
        let seq = numbers(&[(0.0, 0.0), (0.25, 100.0), (1.0, 0.0)]);
        assert_eq!(eval_number(&seq, 0.25), 100.0);
        assert_eq!(eval_number(&seq, 0.125), 50.0);
    }

    #[test]
    fn a_single_keypoint_sequence_is_constant() {
        let seq = numbers(&[(0.0, 3.0)]);
        assert_eq!(eval_number(&seq, 0.0), 3.0);
        assert_eq!(eval_number(&seq, 0.9), 3.0);
    }

    #[test]
    fn an_empty_sequence_evaluates_to_zero() {
        let seq = NumberSequence { keypoints: vec![] };
        assert_eq!(eval_number(&seq, 0.5), 0.0);
    }

    #[test]
    fn a_spread_moves_each_keypoint_that_far_along_its_own_envelope() {
        let seq = enveloped(&[(0.0, 1.0, 0.8), (1.0, 3.0, 0.0)]);
        let at = |t, spread| eval_number_spread(&seq, t, spread);
        for (got, want) in [
            (at(0.0, 1.0), 1.8),
            (at(0.0, -1.0), 0.2),
            (at(0.5, 1.0), 2.4),
            (at(1.0, -1.0), 3.0),
        ] {
            assert!((got - want).abs() < 1e-6, "{got} != {want}");
        }
    }

    #[test]
    fn no_spread_reads_the_plain_values() {
        let seq = enveloped(&[(0.0, 1.0, 0.8), (1.0, 3.0, 0.5)]);
        assert_eq!(eval_number_spread(&seq, 0.25, 0.0), eval_number(&seq, 0.25));
    }

    #[test]
    fn color_interpolates_per_channel_and_linearizes_srgb() {
        let seq = colors(&[(0.0, [0.0, 0.0, 0.0]), (1.0, [1.0, 1.0, 1.0])]);
        assert_eq!(eval_color(&seq, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(eval_color(&seq, 1.0), [1.0, 1.0, 1.0]);
        // Halfway in sRGB space is not halfway in linear space.
        let mid = eval_color(&seq, 0.5);
        assert!(mid[0] > 0.0 && mid[0] < 0.5);
    }
}
