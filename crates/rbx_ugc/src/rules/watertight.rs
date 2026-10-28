use std::collections::HashMap;

use crate::{Finding, Item, Rule, Severity, Value};

const WELD: f32 = 1e-4;

pub(crate) fn watertight(item: &Item, out: &mut Vec<Finding>) {
    for part in &item.parts {
        let Some(geometry) = &part.geometry else {
            continue;
        };
        let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
        let welded: Vec<u32> = geometry
            .positions
            .iter()
            .map(|p| {
                let key = p.map(|c| (c / WELD).round() as i64);
                let next = ids.len() as u32;
                *ids.entry(key).or_insert(next)
            })
            .collect();

        let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
        for tri in &geometry.triangles {
            let Some(corners) = tri
                .iter()
                .map(|&i| welded.get(i as usize).copied())
                .collect::<Option<Vec<u32>>>()
            else {
                continue;
            };
            let (a, b, c) = (corners[0], corners[1], corners[2]);
            if a == b || b == c || a == c {
                continue;
            }
            for (x, y) in [(a, b), (b, c), (c, a)] {
                *edges.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }

        let bad = edges.values().filter(|&&n| n != 2).count();
        if bad > 0 {
            out.push(Finding {
                rule: Rule::Watertight,
                severity: Severity::Warning,
                subject: part.name.clone(),
                measured: Some(Value::Count(bad)),
                limit: Some(Value::Count(0)),
                message: format!(
                    "{bad} open edges: close the holes so the mesh is watertight (Roblox\u{2019}s spec asks for it)."
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetType, BodyScale, Geometry, Part, IDENTITY};

    const CUBE: [[u32; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];

    fn cube_positions() -> Vec<[f32; 3]> {
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ]
    }

    fn run(positions: Vec<[f32; 3]>, triangles: Vec<[u32; 3]>) -> Vec<Finding> {
        let item = Item {
            name: "Cube".into(),
            asset_type: AssetType::Hat,
            scale: BodyScale::Normal,
            parts: vec![Part {
                name: "Handle".into(),
                geometry: Some(Geometry {
                    positions,
                    triangles,
                    ..Default::default()
                }),
                cframe: IDENTITY,
                attachments: vec![],
            }],
            textures: vec![],
            cages: vec![],
        };
        let mut out = Vec::new();
        watertight(&item, &mut out);
        out
    }

    #[test]
    fn a_closed_cube_passes() {
        assert!(run(cube_positions(), CUBE.to_vec()).is_empty());
    }

    #[test]
    fn a_cube_missing_a_triangle_has_three_open_edges() {
        let mut tris = CUBE.to_vec();
        tris.pop();
        let out = run(cube_positions(), tris);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].measured, Some(Value::Count(3)));
    }

    #[test]
    fn a_cube_with_split_vertices_passes_after_welding() {
        let base = cube_positions();
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for tri in CUBE {
            let start = positions.len() as u32;
            positions.extend(tri.iter().map(|&i| base[i as usize]));
            triangles.push([start, start + 1, start + 2]);
        }
        assert!(run(positions, triangles).is_empty());
    }
}
