use super::*;
use rbx_dom::{CFrameData, Vector3Data};

fn nothing(_: &AssetRef) -> Option<Vec<u8>> {
    None
}

fn world() -> (WeakDom, Ref, Ref) {
    let mut dom = WeakDom::new();
    // Services sit at the root, the way a place file loads.
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    dom.take_changes();
    (dom, workspace, storage)
}

fn add(dom: &mut WeakDom, parent: Ref, class: &str, props: Vec<(&str, Variant)>) -> Ref {
    let referent = dom.new_instance(class, class, Some(parent));
    for (name, value) in props {
        dom.set_property(referent, name, value).unwrap();
    }
    referent
}

fn at(x: f32) -> (&'static str, Variant) {
    (
        "CFrame",
        Variant::CFrame(CFrameData {
            position: Vector3Data { x, y: 0.0, z: 0.0 },
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }),
    )
}

fn block(dom: &mut WeakDom, parent: Ref, x: f32) -> Ref {
    add(dom, parent, "Part", vec![at(x)])
}

fn metrics(dom: &WeakDom) -> Metrics {
    Metrics::new(dom, &mut nothing)
}

fn value(report: &Report, referent: Ref) -> f32 {
    report.readout(referent).expect("part is reported").value
}

#[test]
fn grouped_inserts_thousands_separators() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1000), "1,000");
    assert_eq!(grouped(1_234_567), "1,234,567");
}

#[test]
fn bytes_picks_a_unit() {
    assert_eq!(bytes(512), "512 B");
    assert_eq!(bytes(1536), "1.5 KB");
    assert_eq!(bytes(3 * 1024 * 1024), "3.0 MB");
}

#[test]
fn density_sums_instances_per_cell() {
    let (mut dom, workspace, _) = world();
    let a = block(&mut dom, workspace, 1.0);
    let b = block(&mut dom, workspace, 5.0);
    let c = block(&mut dom, workspace, 100.0);
    add(&mut dom, b, "Decal", vec![]);

    let report = metrics(&dom).report(Metric::Density);

    assert_eq!(value(&report, a), 3.0);
    assert_eq!(value(&report, b), 3.0);
    assert_eq!(value(&report, c), 1.0);
}

#[test]
fn triangles_count_primitives_unions_and_missing_meshes() {
    let (mut dom, workspace, _) = world();
    let plain = block(&mut dom, workspace, 0.0);
    let union = add(
        &mut dom,
        workspace,
        "UnionOperation",
        vec![at(0.0), ("TriangleCount", Variant::Int32(500))],
    );
    let mesh = add(
        &mut dom,
        workspace,
        "MeshPart",
        vec![
            at(0.0),
            ("size", Variant::Vector3(Vector3Data { x: 1.0, y: 1.0, z: 1.0 })),
            ("MeshId", Variant::String("rbxassetid://5".to_string())),
        ],
    );

    let report = metrics(&dom).report(Metric::Triangles);

    assert_eq!(value(&report, plain), 12.0);
    assert_eq!(value(&report, union), 500.0);
    assert_eq!(value(&report, mesh), 0.0);
    assert!(report.summary.contains("not counted"), "{}", report.summary);
}

fn collision_of(props: Vec<(&str, Variant)>, class: &str) -> f32 {
    let (mut dom, workspace, _) = world();
    let mut all = vec![at(0.0)];
    all.extend(props);
    let part = add(&mut dom, workspace, class, all);
    value(&metrics(&dom).report(Metric::Collision), part)
}

#[test]
fn collision_scores_fidelity_and_interaction() {
    let anchored = ("Anchored", Variant::Bool(true));
    assert_eq!(
        collision_of(
            vec![anchored.clone(), ("CollisionFidelity", Variant::Enum(3))],
            "MeshPart"
        ),
        8.0
    );
    assert_eq!(
        collision_of(
            vec![anchored.clone(), ("CollisionFidelity", Variant::Enum(2))],
            "MeshPart"
        ),
        1.0
    );
    assert_eq!(
        collision_of(
            vec![anchored.clone(), ("CanCollide", Variant::Bool(false))],
            "Part"
        ),
        0.25
    );
    assert_eq!(
        collision_of(
            vec![
                anchored,
                ("CanCollide", Variant::Bool(false)),
                ("CanTouch", Variant::Bool(false)),
                ("CanQuery", Variant::Bool(false)),
            ],
            "Part"
        ),
        0.0
    );
}

fn welded(first_anchored: bool) -> (Report, Ref, Ref) {
    let (mut dom, workspace, _) = world();
    let a = add(
        &mut dom,
        workspace,
        "Part",
        vec![at(0.0), ("Anchored", Variant::Bool(first_anchored))],
    );
    let b = block(&mut dom, workspace, 4.0);
    add(
        &mut dom,
        workspace,
        "WeldConstraint",
        vec![("Part0", Variant::Ref(a)), ("Part1", Variant::Ref(b))],
    );
    (metrics(&dom).report(Metric::Collision), a, b)
}

#[test]
fn an_unanchored_assembly_doubles_its_collision_cost() {
    let (report, a, b) = welded(false);

    assert_eq!(value(&report, a), 2.0);
    assert_eq!(value(&report, b), 2.0);
    assert!(report
        .readout(a)
        .unwrap()
        .detail
        .contains("unanchored assembly of 2"));
}

#[test]
fn an_assembly_with_an_anchored_part_is_not_doubled() {
    let (report, a, b) = welded(true);

    assert_eq!(value(&report, a), 1.0);
    assert_eq!(value(&report, b), 1.0);
}

#[test]
fn draw_cost_divides_a_batch_among_its_parts() {
    let (mut dom, workspace, _) = world();
    let same: Vec<Ref> = (0..3)
        .map(|i| {
            add(
                &mut dom,
                workspace,
                "Part",
                vec![at(i as f32), ("Material", Variant::Enum(256))],
            )
        })
        .collect();
    let other = add(
        &mut dom,
        workspace,
        "Part",
        vec![at(9.0), ("Material", Variant::Enum(512))],
    );

    let report = metrics(&dom).report(Metric::DrawCost);

    for referent in same {
        assert!((value(&report, referent) - 1.0 / 3.0).abs() < 1e-6);
    }
    assert_eq!(value(&report, other), 1.0);
    assert!(report.summary.contains("about 2 batches"), "{}", report.summary);
}

#[test]
fn every_part_lands_in_one_bin_and_the_worst_list_is_capped_and_sorted() {
    let (mut dom, workspace, storage) = world();
    for i in 0..25 {
        block(&mut dom, workspace, i as f32 * 40.0);
    }
    for _ in 0..3 {
        add(&mut dom, workspace, "Part", vec![at(0.0)]);
    }
    block(&mut dom, storage, 0.0);
    add(&mut dom, workspace, "Terrain", vec![]);

    let report = metrics(&dom).report(Metric::Density);

    assert_eq!(report.parts(), 28);
    let mut binned: Vec<Ref> = report.bins.iter().flat_map(|b| b.parts.clone()).collect();
    binned.sort_unstable();
    binned.dedup();
    assert_eq!(binned.len(), 28);
    assert_eq!(
        report.bins.iter().map(|b| b.parts.len()).sum::<usize>(),
        28
    );
    assert_eq!(report.worst.len(), WORST);
    let values: Vec<f32> = report.worst.iter().map(|&r| value(&report, r)).collect();
    assert!(values.windows(2).all(|w| w[0] >= w[1]));
}

#[test]
fn patching_a_property_updates_the_figures() {
    let (mut dom, workspace, _) = world();
    let a = add(&mut dom, workspace, "Part", vec![at(0.0)]);
    let b = add(&mut dom, workspace, "Part", vec![at(1.0)]);
    let mut metrics = metrics(&dom);
    assert_eq!(value(&metrics.report(Metric::DrawCost), a), 0.5);

    dom.set_property(b, "Material", Variant::Enum(512)).unwrap();
    let changes = dom.take_changes();
    metrics.patch(&dom, &changes, &mut nothing);

    let report = metrics.report(Metric::DrawCost);
    assert_eq!(value(&report, a), 1.0);
    assert_eq!(value(&report, b), 1.0);
}

#[test]
fn patching_a_removal_drops_the_part() {
    let (mut dom, workspace, _) = world();
    let a = block(&mut dom, workspace, 0.0);
    let b = block(&mut dom, workspace, 1.0);
    let mut metrics = metrics(&dom);

    dom.remove(a);
    let changes = dom.take_changes();
    metrics.patch(&dom, &changes, &mut nothing);

    let report = metrics.report(Metric::DrawCost);
    assert!(report.readout(a).is_none());
    assert!(report.readout(b).is_some());
    assert_eq!(metrics.len(), 1);
}

#[test]
fn patching_a_reparent_into_the_workspace_adds_the_part() {
    let (mut dom, workspace, storage) = world();
    let part = block(&mut dom, storage, 0.0);
    let mut metrics = metrics(&dom);
    assert!(metrics.is_empty());

    dom.set_parent(part, Some(workspace));
    let changes = dom.take_changes();
    metrics.patch(&dom, &changes, &mut nothing);

    assert!(metrics.report(Metric::Density).readout(part).is_some());
}

/// What measuring costs on a real place: `marked.rbxl` from the fixtures
/// repository `RBX_FIXTURES` names, read against the real asset cache, then
/// the same place with its parts copied `PERF_COPIES` times over (default
/// 100) — and what a frame costs with the heat tint on and off.
/// `RBX_FIXTURES=… cargo test --release -p rbx_viewer --lib perf::tests::measures -- --ignored --nocapture`
#[test]
#[ignore = "needs RBX_FIXTURES, a filled asset cache and a GPU"]
fn measures_a_real_place() {
    let path = std::path::Path::new(&std::env::var("RBX_FIXTURES").expect("RBX_FIXTURES"))
        .join("places/marked.rbxl");
    let dom = crate::read_place(&path).expect("marked.rbxl");
    let copies: usize = std::env::var("PERF_COPIES").map_or(100, |n| n.parse().unwrap());
    bench("marked.rbxl", dom.clone());
    bench(&format!("marked.rbxl x{copies}"), scaled(dom, copies));
}

/// `dom` with every part under its `Workspace` copied `copies - 1` more
/// times, each copy 600 studs further along X.
fn scaled(mut dom: WeakDom, copies: usize) -> WeakDom {
    let database = ReflectionDatabase::shared();
    let workspace = dom
        .root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).unwrap().class() == "Workspace")
        .unwrap();
    let parts: Vec<Ref> = crate::scene::workspace_descendants(&dom, database)
        .filter(|&r| is_drawable(&dom, database, r))
        .collect();
    for copy in 1..copies {
        for &part in &parts {
            let instance = dom.get(part).unwrap();
            let (class, name) = (instance.class().to_string(), instance.name().to_string());
            let mut properties = instance.properties().clone();
            if let Some(Variant::CFrame(cframe)) = properties.get_mut("CFrame") {
                cframe.position.x += 600.0 * copy as f32;
            }
            let copy = dom.new_instance(&class, &name, Some(workspace));
            for (key, value) in properties {
                dom.set_property(copy, &key, value).unwrap();
            }
        }
    }
    dom.take_changes();
    dom
}

fn bench(label: &str, mut dom: WeakDom) {
    use std::time::{Duration, Instant};
    let cache = rbx_assets::AssetCache::new(None).expect("an asset cache");
    let mut fetch = |asset: &AssetRef| match asset {
        AssetRef::Id(id) => cache.get_id(*id),
        AssetRef::Native(path) => cache.get_native(path),
        _ => None,
    };
    let started = Instant::now();
    let mut metrics = Metrics::new(&dom, &mut fetch);
    println!("[{label}] build: {} parts in {:?}", metrics.len(), started.elapsed());
    for metric in Metric::ALL {
        let started = Instant::now();
        let report = metrics.report(metric);
        println!("[{label}] {metric:?} report: {:?} — {}", started.elapsed(), report.summary);
    }

    // One drag step's worth of edits: a `CFrame` written on 50 parts.
    let parts: Vec<Ref> = metrics.entries.keys().copied().take(50).collect();
    for &part in &parts {
        let cframe = dom.get(part).unwrap().properties()["CFrame"].clone();
        dom.set_property(part, "CFrame", cframe).unwrap();
    }
    let changes = dom.take_changes();
    let started = Instant::now();
    metrics.patch(&dom, &changes, &mut fetch);
    let patched = started.elapsed();
    let started = Instant::now();
    let report = metrics.report(Metric::Triangles);
    println!(
        "[{label}] patch of {} parts: {patched:?}, report after it: {:?}",
        parts.len(),
        started.elapsed()
    );

    // Frame time, heat off and on, 1280x720, after the assets settle.
    let mut headless = crate::Headless::from_dom(&dom, true).expect("a GPU");
    let settling = Instant::now();
    while (headless.assets_in_flight() > 0 || headless.swap_assets())
        && settling.elapsed() < Duration::from_secs(120)
    {}
    let frames = |headless: &mut crate::Headless| {
        for _ in 0..60 {
            headless.render_frame(1280, 720).unwrap();
        }
        let mut times: Vec<Duration> = (0..120)
            .map(|_| {
                let started = Instant::now();
                headless.render_frame(1280, 720).unwrap();
                started.elapsed()
            })
            .collect();
        headless.take_frame().unwrap();
        times.sort();
        (times[times.len() / 2], times[times.len() * 95 / 100])
    };
    // Off, on, off again: the first off still pays for whatever the load
    // left settling, which the second does not.
    let off = frames(&mut headless);
    let started = Instant::now();
    headless.set_heat(report.bins.clone());
    let set = started.elapsed();
    let on = frames(&mut headless);
    headless.set_heat(Vec::new());
    let again = frames(&mut headless);
    println!(
        "[{label}] frame median/p95: off {:?}/{:?}, heat on {:?}/{:?}, off again {:?}/{:?} (set_heat {set:?})",
        off.0, off.1, on.0, on.1, again.0, again.1
    );
}
