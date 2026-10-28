//! Behaviour Roblox does not document, held to what real Studio was
//! measured doing (`tests/studio_graph_probe.rs`; the numbers and how they
//! were taken are in `docs/research/animation-graph-measured.md`).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use super::semantics::{output, param, set, set_pin};
use super::*;

type Weights = &'static [f32];

/// Blend2D weights for `points`, one row per measured `(target, weights)`.
fn blend2d(polar: bool, points: &[(f64, f64)], rows: &[((f64, f64), Weights)]) {
    let mut g = Graph::default();
    let blend = g.add(NodeKind::Blend2D);
    if polar {
        set(&mut g, blend, "InputMode", Value::Text("Polar".into()));
    }
    for (k, &(x, y)) in points.iter().enumerate() {
        let c = clip_node(&mut g, "idle", "Loop");
        g.connect_new(c, blend);
        set_pin(&mut g, blend, k, "X", Prop::Value(Value::Number(x)));
        set_pin(&mut g, blend, k, "Y", Prop::Value(Value::Number(y)));
    }
    output(&mut g, blend);
    for &((x, y), studio) in rows {
        let mut graph = g.clone();
        set(&mut graph, blend, "X", Value::Number(x));
        set(&mut graph, blend, "Y", Value::Number(y));
        let p = Player::new(graph, &resolve);
        let ours: Vec<f32> = p.blend_weights(blend).iter().map(|w| w.1).collect();
        for (a, b) in ours.iter().zip(studio) {
            assert!((a - b).abs() < 1e-4, "({x}, {y}): {ours:?} vs {studio:?}");
        }
    }
}

#[test]
fn blend2d_polar_weights_match_studio() {
    let ring = [
        (0., 0.),
        (0., 1.),
        (FRAC_PI_2, 1.),
        (PI, 1.),
        (-FRAC_PI_2, 1.),
    ];
    blend2d(
        true,
        &ring,
        &[
            ((FRAC_PI_4, 0.5), &[0.33333, 0.33333, 0.33333, 0., 0.]),
            ((3. * FRAC_PI_4, 0.7), &[0.23077, 0., 0.38462, 0.38462, 0.]),
            ((0.3, 0.2), &[0.67171, 0.16793, 0.16036, 0., 0.]),
            ((5. * FRAC_PI_4, 1.), &[0., 0., 0., 0.5, 0.5]),
        ],
    );
    // Past the last input the band exceeds 1, and Studio keeps it.
    blend2d(
        true,
        &[(0., 1.), (1., 1.), (2.5, 1.)],
        &[
            ((0.25, 1.), &[0.75, 0.25, 0.]),
            ((4.5, 1.), &[0.48766, 0., 0.51234]),
        ],
    );
    blend2d(
        true,
        &[(0., 0.), (0., 1.), (1.2, 2.), (2.5, 0.7), (-1., 1.5)],
        &[
            ((0.6, 1.), &[0., 0.53582, 0.46418, 0., 0.]),
            ((0.3, 1.8), &[0., 0.71060, 0.28940, 0., 0.]),
            ((1.8, 1.2), &[0., 0., 0.51990, 0.48010, 0.]),
            ((-0.5, 0.6), &[0.13072, 0.50327, 0., 0., 0.36601]),
            ((3.5, 1.), &[0., 0., 0., 0.64044, 0.35956]),
            ((1., 0.4), &[0.47039, 0.29563, 0.21952, 0.01446, 0.]),
        ],
    );
}

#[test]
fn blend2d_cartesian_weights_match_studio() {
    blend2d(
        false,
        &[(0., 0.), (1., 0.), (0.3, 1.), (-1., 0.5), (0.5, -0.8)],
        &[
            ((0.2, 0.2), &[0.63456, 0.16667, 0.19878, 0., 0.]),
            ((0.6, 0.4), &[0.28571, 0.38830, 0.32598, 0., 0.]),
            ((2., 2.), &[0., 0.12752, 0.87248, 0., 0.]),
            ((-0.2, 0.8), &[0.24306, 0., 0.46431, 0.29263, 0.]),
        ],
    );
}

/// The first 40 inputs a RandomSequence with `seed` and these weights plays.
fn picks(seed: f64, weights: &[f64]) -> String {
    let mut g = Graph::default();
    let r = g.add(NodeKind::RandomSequence);
    set(&mut g, r, "Seed", Value::Number(seed));
    set(&mut g, r, "DefaultTransitionDuration", Value::Number(0.));
    for (k, &w) in weights.iter().enumerate() {
        let c = clip_node(&mut g, "idle", "OnceAndHold");
        g.connect_new(c, r);
        set_pin(&mut g, r, k, "Weight", Prop::Value(Value::Number(w)));
    }
    output(&mut g, r);
    let mut p = Player::new(g, &resolve);
    p.step(0.);
    (0..40)
        .map(|_| {
            let pick = p.active(r).unwrap().to_string();
            p.step(1.);
            pick
        })
        .collect()
}

#[test]
fn random_sequence_picks_match_studio() {
    let three = [1., 1., 1.];
    assert_eq!(
        picks(7., &three),
        "0020122120101001001122201102022201122002"
    );
    assert_eq!(
        picks(7., &[1., 2., 3.]),
        "0121122121212001101222211102122211122102"
    );
    assert_eq!(
        picks(42., &[1.; 5]),
        "1340332202000241303300434410040313202012"
    );
    assert_eq!(
        picks(0., &[1., 1.]),
        "1111111101100010100011101011000100111011"
    );
    assert_eq!(
        picks(-2., &three),
        "1012121111202212012002222012120110111210"
    );
    assert_eq!(picks(7.5, &three), picks(8., &three));
}

#[test]
fn cubic_v2_transitions_follow_studio_s_curve() {
    // Real Studio's blend weight at each 1/30 s of a one-second transition.
    let studio = [
        0.00015, 0.00119, 0.004, 0.00948, 0.01852, 0.032, 0.05081, 0.07585, 0.108, 0.14815,
        0.19719, 0.256, 0.32548, 0.40652, 0.5,
    ];
    for (k, w) in studio.iter().enumerate() {
        let t = (k + 1) as f32 / 30.;
        assert!((curve_at(Style::CubicV2, t) - w).abs() < 1e-4, "{t}");
        assert!((curve_at(Style::CubicV2, 1. - t) - (1. - w)).abs() < 1e-4);
    }
}

fn select_graph() -> (Graph, usize) {
    let mut g = Graph::default();
    param(&mut g, "pick", ParamKind::Text);
    let sel = g.add(NodeKind::Select);
    for id in ["idle", "run"] {
        let n = clip_node(&mut g, id, "Loop");
        g.connect(n, sel, id);
    }
    g.nodes[sel]
        .props
        .insert("Selection".into(), Prop::Param("pick".into()));
    set(&mut g, sel, "DefaultTransitionDuration", Value::Number(1.));
    output(&mut g, sel);
    (g, sel)
}

#[test]
fn a_selection_naming_no_input_plays_none() {
    let (g, sel) = select_graph();
    let mut p = Player::new(g, &resolve);
    assert!(p.step(0.5).is_empty(), "nothing plays from the start");
    assert_eq!(p.active(sel), None);
    p.set("pick", Value::Text("idle".into()));
    p.step(1.);
    // Studio: 1 → 0.9 three Linear steps after Selection went to "Nope".
    p.set("pick", Value::Text("Nope".into()));
    let pose = p.step(0.1);
    assert!((x(&pose, "Torso") - 0.9 * 0.1).abs() < 1e-3, "{pose:?}");
    assert!(p.step(1.).is_empty(), "faded out to the rest pose");
}

#[test]
fn a_priority_select_with_no_trigger_plays_none() {
    let mut g = Graph::default();
    let ps = g.add(NodeKind::PrioritySelect);
    for id in ["idle", "run"] {
        let n = clip_node(&mut g, id, "Loop");
        g.connect_new(n, ps);
    }
    output(&mut g, ps);
    let mut p = Player::new(g, &resolve);
    assert!(p.step(0.1).is_empty());
    assert!(p.step(1.).is_empty());
}

#[test]
fn a_backwards_trim_freezes_at_trim_end() {
    let mut g = Graph::default();
    let clip = clip_node(&mut g, "long", "Loop");
    set(&mut g, clip, "Trim", Value::Bool(true));
    set(&mut g, clip, "TrimStart", Value::Number(0.3));
    set(&mut g, clip, "TrimEnd", Value::Number(0.1));
    output(&mut g, clip);
    let mut p = Player::new(g, &resolve);
    for _ in 0..12 {
        assert!((x(&p.step(1. / 15.), "Leg") - 30.1).abs() < 1e-4);
    }
}

#[test]
fn a_retriggered_one_shot_restarts() {
    let mut g = Graph::default();
    param(&mut g, "go", ParamKind::Bool);
    let shot = g.add(NodeKind::OneShot);
    set(&mut g, shot, "TransitionInDuration", Value::Number(0.));
    set(&mut g, shot, "TransitionOutDuration", Value::Number(0.));
    g.nodes[shot]
        .props
        .insert("Trigger".into(), Prop::Param("go".into()));
    let base = clip_node(&mut g, "idle", "Loop");
    let wave = clip_node(&mut g, "wave", "OnceAndHold");
    g.connect(base, shot, "Base");
    g.connect(wave, shot, "OneShot");
    output(&mut g, shot);
    let mut p = Player::new(g, &resolve);
    p.set("go", Value::Bool(true));
    p.step(0.2);
    p.set("go", Value::Bool(false));
    p.step(1. / 30.);
    p.set("go", Value::Bool(true));
    let arm = x(&p.step(1. / 30.), "Arm");
    assert!((arm - (20. + 1. / 30.)).abs() < 1e-4, "{arm}");
}
