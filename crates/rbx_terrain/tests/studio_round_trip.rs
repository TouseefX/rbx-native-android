//! A round trip through real Roblox Studio. `write_place_for_studio` saves a
//! place with terrain we wrote and the command-bar Lua that dumps what Studio
//! reads back into `Workspace.VoxelDump`; after running it and saving in
//! Studio, `studio_read_what_we_wrote` checks Studio's dump and Studio's own
//! re-saved `SmoothGrid` against our voxels. Both need Studio, so both are
//! ignored by a plain `cargo test`.

use rbx_dom::{Variant, WeakDom};
use rbx_terrain::edit::region::StudBox;
use rbx_terrain::generate::{generate, Biome, GenerateSettings};
use rbx_terrain::{Cell, Material, VoxelGrid};

/// Voxel bounds of everything we write, inclusive min, exclusive max.
const MIN: [i32; 3] = [-12, -2, -12];
const MAX: [i32; 3] = [12, 12, 12];

fn our_grid() -> VoxelGrid {
    let mut grid = VoxelGrid::new();
    let settings = GenerateSettings {
        biomes: vec![Biome::Hills, Biome::Mountains, Biome::Water, Biome::Dunes],
        blending: 0.4,
        caves: false,
        biome_size: 64.0,
        seed: 154,
    };
    let region = StudBox {
        min: [-48.0, -8.0, -48.0],
        max: [48.0, 32.0, 48.0],
    };
    generate(&mut grid, &region, &settings);
    // One row per material at four occupancies, floating above the hills.
    for (i, material) in Material::ALL.iter().skip(1).enumerate() {
        for (j, fraction) in [0.25f32, 0.5, 0.75, 1.0].into_iter().enumerate() {
            let voxel = [i as i32 - 11, 11, j as i32 * 2 - 4];
            grid.set(voxel, Cell::with_fraction(*material, fraction));
        }
    }
    grid
}

fn terrain_ref(dom: &WeakDom) -> rbx_dom::Ref {
    let mut stack = dom.root_refs().to_vec();
    while let Some(referent) = stack.pop() {
        let instance = dom.get(referent).unwrap();
        stack.extend(instance.children().iter().copied());
        if instance.class() == "Terrain" {
            return referent;
        }
    }
    panic!("no Terrain");
}

fn blob(dom: &WeakDom, property: &str) -> Vec<u8> {
    match dom
        .get(terrain_ref(dom))
        .unwrap()
        .properties()
        .get(property)
    {
        Some(Variant::String(text)) => text.as_bytes().to_vec(),
        Some(Variant::Unknown { raw, .. }) => raw.clone(),
        other => panic!("{property} is {other:?}"),
    }
}

fn out_dir() -> std::path::PathBuf {
    std::env::var("RBX_TERRAIN_OUT")
        .expect("RBX_TERRAIN_OUT")
        .into()
}

/// Lua for Studio's command bar: reads every voxel in our bounds and stores
/// the non-air ones as `x,y,z,Material,occupancy` lines.
fn dump_lua() -> String {
    let lo = MIN.map(|v| v * 4);
    let hi = MAX.map(|v| v * 4);
    format!(
        r#"local r = Region3.new(Vector3.new({}, {}, {}), Vector3.new({}, {}, {}))
local m, o = workspace.Terrain:ReadVoxels(r, 4)
local s = m.Size
local out = {{}}
for x = 1, s.X do for y = 1, s.Y do for z = 1, s.Z do
  local mat = m[x][y][z]
  if mat ~= Enum.Material.Air then
    table.insert(out, string.format("%d,%d,%d,%s,%.5f", x - 1 + {}, y - 1 + {}, z - 1 + {}, mat.Name, o[x][y][z]))
  end
end end end
local v = workspace:FindFirstChild("VoxelDump") or Instance.new("StringValue")
v.Name = "VoxelDump"
v.Value = table.concat(out, "\n")
v.Parent = workspace
print("VoxelDump", #out, "voxels", s)
"#,
        lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], MIN[0], MIN[1], MIN[2]
    )
}

#[test]
#[ignore = "writes a place for a real Studio round trip (RBX_TERRAIN_OUT)"]
fn write_place_for_studio() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let base = std::fs::read(format!("{manifest}/../../assets/tests/TestPlace.rbxl")).unwrap();
    let mut dom = rbx_binary::deserialize(&base).unwrap();
    let terrain = terrain_ref(&dom);
    let grid = our_grid();
    let set = |dom: &mut WeakDom, name: &str, raw: Vec<u8>| {
        dom.set_property(terrain, name, Variant::Unknown { type_id: 1, raw })
            .unwrap();
    };
    set(
        &mut dom,
        "SmoothGrid",
        rbx_terrain::smooth_grid::encode(&grid),
    );
    set(
        &mut dom,
        "PhysicsGrid",
        rbx_terrain::physics_grid::encode(&grid),
    );
    // Above the hills, looking down at them: the stock camera sits inside one.
    let camera = (0..dom.next_ref())
        .map(rbx_dom::Ref::new)
        .find(|&r| dom.get(r).is_some_and(|i| i.class() == "Camera"))
        .expect("TestPlace has a Camera");
    let (c, s) = (0.8575, 0.5145);
    let view = rbx_dom::CFrameData {
        position: rbx_dom::Vector3Data {
            x: 0.0,
            y: 70.0,
            z: 100.0,
        },
        rotation: [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c],
    };
    dom.set_property(camera, "CFrame", Variant::CFrame(view))
        .unwrap();
    let dir = out_dir();
    std::fs::write(
        dir.join("terrain154.rbxl"),
        rbx_binary::serialize(&dom).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("terrain154_dump.lua"), dump_lua()).unwrap();
    eprintln!(
        "{} voxels written",
        grid.voxels().filter(|(_, c)| !c.is_air()).count()
    );
}

#[test]
#[ignore = "needs RBX_TERRAIN_OUT/terrain154_studio.rbxl, saved by real Studio"]
fn studio_read_what_we_wrote() {
    let saved = std::fs::read(out_dir().join("terrain154_studio.rbxl")).unwrap();
    let dom = rbx_binary::deserialize(&saved).unwrap();
    let ours = our_grid();

    // What Studio's ReadVoxels gave back, voxel by voxel.
    let workspace = dom
        .root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).unwrap().class() == "Workspace")
        .unwrap();
    let dump = dom
        .get(workspace)
        .unwrap()
        .children()
        .iter()
        .filter_map(|&c| dom.get(c))
        .find(|i| i.name() == "VoxelDump")
        .expect("run terrain154_dump.lua in Studio first");
    let Some(Variant::String(text)) = dom.get(dump.referent()).unwrap().properties().get("Value")
    else {
        panic!("VoxelDump.Value is not a string");
    };
    let mut seen = 0;
    let mut worst = 0f32;
    for line in text.lines() {
        let f: Vec<&str> = line.split(',').collect();
        let voxel = [0, 1, 2].map(|a| f[a].parse::<i32>().unwrap());
        let cell = ours.get(voxel);
        assert_eq!(cell.material.name(), f[3], "material at {voxel:?}");
        let occupancy: f32 = f[4].parse().unwrap();
        worst = worst.max((occupancy - cell.fraction()).abs());
        seen += 1;
    }
    let solid = ours
        .voxels()
        .filter(|(v, c)| !c.is_air() && (0..3).all(|a| v[a] >= MIN[a] && v[a] < MAX[a]))
        .count();
    eprintln!("Studio read {seen} voxels, we wrote {solid}; worst occupancy gap {worst}");
    assert_eq!(seen, solid, "Studio sees the same non-air voxels");
    assert!(worst <= 1.0 / 255.0, "occupancy within one step");

    // Studio's own save of the same terrain.
    let studio_smooth = blob(&dom, "SmoothGrid");
    let studio_grid = rbx_terrain::smooth_grid::decode(&studio_smooth).unwrap();
    let mismatched = ours
        .voxels()
        .chain(studio_grid.voxels())
        .filter(|(v, _)| ours.get(*v) != studio_grid.get(*v))
        .count();
    eprintln!(
        "Studio SmoothGrid {} bytes, ours {}; {mismatched} voxels differ; byte-identical: {}",
        studio_smooth.len(),
        rbx_terrain::smooth_grid::encode(&ours).len(),
        studio_smooth == rbx_terrain::smooth_grid::encode(&ours)
    );
    assert_eq!(mismatched, 0);
    assert_eq!(
        blob(&dom, "PhysicsGrid"),
        rbx_terrain::physics_grid::encode(&studio_grid),
        "our PhysicsGrid rule matches what Studio saved"
    );
}
