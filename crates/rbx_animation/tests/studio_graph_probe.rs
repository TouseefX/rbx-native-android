//! Probes for Animation Graph behaviour Roblox does not document.
//! `write_probe_for_studio` saves a place with a four-joint rig, clips that
//! each move one joint along one axis, and one graph per question (Polar
//! Blend2D, RandomSequence seeding, transition curves, Select, Mask, Trim),
//! plus the command-bar Lua that plays each graph on Studio's Animator and
//! prints what it sees to the Output (and so to Studio's log) as
//! `GP|<case>|<sample>|...` lines. The numbers it measured are in
//! `docs/research/animation-graph-measured.md`. Needs Studio.

use glam::Vec3;
use rbx_animation::graph::{self, Value};
use rbx_animation::Frame;
use rbx_dom::{Ref, Variant, WeakDom};

const DT: f32 = 1. / 30.;
const JOINTS: usize = 4;

#[path = "studio_graph_probe/cases.rs"]
mod cases;
use cases::{cases, Case, Read, PICKS};
/// A KeyframeSequence with a key per `(time, positions)`, one position per
/// joint `P0..`.
fn sequence(
    dom: &mut WeakDom,
    parent: Ref,
    name: &str,
    looped: bool,
    keys: &[(f32, [Vec3; JOINTS])],
) {
    let ks = dom.new_instance("KeyframeSequence", name, Some(parent));
    dom.set_property(ks, "Loop", Variant::Bool(looped)).unwrap();
    dom.set_property(ks, "Priority", Variant::Enum(2)).unwrap();
    for (time, at) in keys {
        let kf = dom.new_instance("Keyframe", "Keyframe", Some(ks));
        dom.set_property(kf, "Time", Variant::Float32(*time))
            .unwrap();
        let root = dom.new_instance("Pose", "Root", Some(kf));
        dom.set_property(root, "CFrame", Variant::CFrame(Frame::IDENTITY.to_data()))
            .unwrap();
        dom.set_property(root, "Weight", Variant::Float32(0.))
            .unwrap();
        for (j, p) in at.iter().enumerate() {
            let pose = dom.new_instance("Pose", &format!("P{j}"), Some(root));
            dom.set_property(pose, "CFrame", Variant::CFrame(Frame::at(*p).to_data()))
                .unwrap();
            dom.set_property(pose, "Weight", Variant::Float32(1.))
                .unwrap();
        }
    }
}

fn write_clips(dom: &mut WeakDom, folder: Ref) {
    for k in 0..JOINTS * 3 {
        let mut at = [Vec3::ZERO; JOINTS];
        at[k / 3][k % 3] = 1.;
        sequence(dom, folder, &format!("T{k}"), true, &[(0., at), (1., at)]);
    }
    for k in 0..5 {
        let x = (k + 1) as f32;
        let mut start = [Vec3::ZERO; JOINTS];
        let mut end = start;
        start[0] = Vec3::new(x, 0., 0.);
        end[0] = Vec3::new(x, 1., 0.);
        sequence(
            dom,
            folder,
            &format!("R{k}"),
            false,
            &[(0., start), (0.5, end)],
        );
    }
    let both = [Vec3::X, Vec3::X, Vec3::ZERO, Vec3::ZERO];
    sequence(dom, folder, "Both", true, &[(0., both), (1., both)]);
}

fn lua_value(v: &Value) -> String {
    match v {
        Value::Number(n) => format!("{n}"),
        Value::Bool(b) => format!("{b}"),
        other => format!("\"{}\"", other.text()),
    }
}

fn studio_lua(cases: &[Case]) -> String {
    let mut list = String::new();
    for case in cases {
        let body = match &case.read {
            Read::Picks => format!("picks={}", PICKS),
            Read::Blend(samples) => {
                let s: Vec<String> = samples
                    .iter()
                    .map(|(p, n)| {
                        let p: Vec<String> = p
                            .iter()
                            .map(|(k, v)| format!("{k}={}", lua_value(v)))
                            .collect();
                        format!("{{p={{{}}},n={n}}}", p.join(","))
                    })
                    .collect();
                format!("s={{{}}}", s.join(","))
            }
        };
        list += &format!("{{g=\"{}\",{body}}},\n", case.graph.name);
    }
    format!(
        r#"local KSP = game:GetService("KeyframeSequenceProvider")
local rig = workspace.ProbeRig
local animator = rig.AnimationController.Animator
local joints = {{}}
for j = 0, {last} do joints[j + 1] = rig["P" .. j].J end
local ids = {{}}
for _, ks in workspace.ProbeClips:GetChildren() do ids[ks.Name] = KSP:RegisterKeyframeSequence(ks) end
for _, d in workspace.ProbeDefs:GetDescendants() do
  if d:IsA("AnimationNodeDefinition") and ids[d:GetAttribute("AnimationId")] then d:SetAttribute("AnimationId", ids[d:GetAttribute("AnimationId")]) end
end
local function f(x) return string.format("%.5f", x) end
local function row()
  local c = {{}}
  for _, j in joints do local p = j.Transform.Position table.insert(c, f(p.X)) table.insert(c, f(p.Y)) table.insert(c, f(p.Z)) end
  return table.concat(c, ",")
end
local cases = {{
{list}}}
for _, case in cases do
  for _, t in animator:GetPlayingAnimationTracks() do t:Stop(0) end
  animator:StepAnimations(0)
  local anim = Instance.new("Animation")
  anim.AnimationId = KSP:RegisterKeyframeSequence(workspace.ProbeDefs[case.g])
  local track = animator:LoadAnimation(anim)
  track:Play(0)
  task.wait(1)
  if case.picks then
    local picks, lastx, lasty, step = {{}}, nil, nil, 0
    while #picks < case.picks and step < case.picks * 20 do
      animator:StepAnimations({DT})
      step += 1
      local p = joints[1].Transform.Position
      local x = math.floor(p.X + 0.5)
      if x ~= lastx or p.Y < lasty - 0.5 then table.insert(picks, (x - 1) .. "@" .. step) end
      lastx, lasty = x, p.Y
    end
    print("GP|" .. case.g .. "|picks|" .. table.concat(picks, " "))
  else
    for i, s in case.s do
      for k, v in s.p do track:SetParameter(k, v) end
      for _ = 1, s.n do animator:StepAnimations({DT}) end
      print("GP|" .. case.g .. "|" .. (i - 1) .. "|" .. row())
    end
  end
  track:Stop(0)
end
print("GP|done")
"#,
        last = JOINTS - 1,
        DT = DT
    )
}

#[test]
#[ignore = "writes a place for a real Studio measurement (RBX_GRAPH_OUT)"]
fn write_probe_for_studio() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let base = std::fs::read(format!("{manifest}/../../assets/tests/TestPlace.rbxl")).unwrap();
    let mut dom = rbx_binary::deserialize(&base).unwrap();
    let dir: std::path::PathBuf = std::env::var("RBX_GRAPH_OUT")
        .expect("RBX_GRAPH_OUT")
        .into();
    // An empty XML place for Studio to save an Instance attribute into.
    std::fs::write(dir.join("entry.rbxlx"), rbx_xml::serialize(&dom).unwrap()).unwrap();
    let workspace = dom
        .root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).unwrap().class() == "Workspace")
        .unwrap();

    let rig = dom.new_instance("Model", "ProbeRig", Some(workspace));
    let part = |dom: &mut WeakDom, name: &str| {
        let p = dom.new_instance("Part", name, Some(rig));
        let at = Frame::at(Vec3::new(0., 10., 0.)).to_data();
        dom.set_property(p, "CFrame", Variant::CFrame(at)).unwrap();
        dom.set_property(p, "Anchored", Variant::Bool(name == "Root"))
            .unwrap();
        p
    };
    let root = part(&mut dom, "Root");
    dom.set_property(rig, "PrimaryPart", Variant::Ref(root))
        .unwrap();
    for j in 0..JOINTS {
        let p = part(&mut dom, &format!("P{j}"));
        let motor = dom.new_instance("Motor6D", "J", Some(p));
        dom.set_property(motor, "Part0", Variant::Ref(root))
            .unwrap();
        dom.set_property(motor, "Part1", Variant::Ref(p)).unwrap();
    }
    let controller = dom.new_instance("AnimationController", "AnimationController", Some(rig));
    dom.new_instance("Animator", "Animator", Some(controller));

    let clips = dom.new_instance("Folder", "ProbeClips", Some(workspace));
    write_clips(&mut dom, clips);
    let defs = dom.new_instance("Folder", "ProbeDefs", Some(workspace));
    let cases = cases();
    for case in &cases {
        graph::dom::write(&mut dom, Some(defs), &case.graph);
    }

    std::fs::write(dir.join("probe.rbxl"), rbx_binary::serialize(&dom).unwrap()).unwrap();
    std::fs::write(dir.join("probe.lua"), studio_lua(&cases)).unwrap();
}
