use crate::spec::{self, BodyGroup, Category};
use crate::{AssetType, CFrame, Finding, Item, Rule, Severity, Value};

const SLACK: f32 = 1e-3;

pub(crate) fn size(item: &Item, out: &mut Vec<Finding>) {
    match item.asset_type.category() {
        Category::Rigid | Category::Layered => accessory(item, out),
        Category::Body => body(item, out),
    }
}

fn to_parent(f: &CFrame, p: [f32; 3]) -> [f32; 3] {
    let row = |i: usize| f[3 + 3 * i] * p[0] + f[4 + 3 * i] * p[1] + f[5 + 3 * i] * p[2] + f[i];
    [row(0), row(1), row(2)]
}

fn to_local(f: &CFrame, p: [f32; 3]) -> [f32; 3] {
    let v = [p[0] - f[0], p[1] - f[1], p[2] - f[2]];
    let col = |j: usize| f[3 + j] * v[0] + f[6 + j] * v[1] + f[9 + j] * v[2];
    [col(0), col(1), col(2)]
}

fn note(rule: Rule, subject: &str, message: String) -> Finding {
    Finding {
        rule,
        severity: Severity::Note,
        subject: subject.to_string(),
        measured: None,
        limit: None,
        message,
    }
}

fn accessory(item: &Item, out: &mut Vec<Finding>) {
    let Some(part) = item.parts.first() else {
        return;
    };
    // A source file has neither studs nor attachments yet; say so whether or
    // not the attachment is there.
    if part.geometry.as_ref().is_some_and(|g| !g.in_studs) {
        out.push(note(
            Rule::ItemSize,
            &part.name,
            "The size of this mesh is checked once the file is imported, because its units are not known."
                .to_string(),
        ));
        return;
    }
    let allowed = item.asset_type.attachments();
    let Some(att) = part
        .attachments
        .iter()
        .find(|a| allowed.contains(&a.name.as_str()))
    else {
        return;
    };
    let label = item.asset_type.label();
    let Some(bounds) = spec::item_bounds(item.asset_type, item.scale, &att.name) else {
        out.push(note(
            Rule::ItemSize,
            &part.name,
            format!("Roblox publishes no size limit for {label}, so it was not checked."),
        ));
        return;
    };
    let Some(geometry) = &part.geometry else {
        return;
    };
    let mut max = [0.0f32; 3];
    for p in &geometry.positions {
        let q = to_local(&att.cframe, *p);
        for i in 0..3 {
            max[i] = max[i].max((q[i] - bounds.offset[i]).abs());
        }
    }
    if (0..3).all(|i| max[i] <= bounds.size[i] / 2.0 + SLACK) {
        return;
    }
    let [w, h, d] = bounds.size;
    let who = if item.asset_type.category() == Category::Rigid {
        format!("{label} items for {} bodies", item.scale.label())
    } else {
        format!("{label} items")
    };
    out.push(Finding {
        rule: Rule::ItemSize,
        severity: Severity::Error,
        subject: part.name.clone(),
        measured: Some(Value::Studs([2.0 * max[0], 2.0 * max[1], 2.0 * max[2]])),
        limit: Some(Value::Studs(bounds.size)),
        message: format!(
            "{who} must fit {w:.2} \u{d7} {h:.2} \u{d7} {d:.2} studs around {}; scale it down or move it closer to the attachment.",
            att.name
        ),
    });
}

fn extent(points: &[[f32; 3]]) -> Option<[f32; 3]> {
    let first = points.first()?;
    let mut lo = *first;
    let mut hi = *first;
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    Some([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]])
}

fn body(item: &Item, out: &mut Vec<Finding>) {
    let mut unknown_units = false;
    let mut usable = Vec::new();
    for part in &item.parts {
        match &part.geometry {
            Some(g) if g.in_studs => usable.push((part, g)),
            Some(_) => unknown_units = true,
            None => {}
        }
    }
    if unknown_units {
        out.push(note(
            Rule::BodyPartSize,
            &item.name,
            "Some body parts are not measured in studs, so their size was not checked until the file is imported."
                .to_string(),
        ));
    }
    // Limits are per body asset: each arm and each leg is measured on its own.
    for (asset, names, _) in spec::BODY_ASSET_TRIANGLES {
        if item.asset_type == AssetType::DynamicHead && asset != "DynamicHead" {
            continue;
        }
        let members: Vec<_> = names
            .iter()
            .filter_map(|n| usable.iter().find(|(p, _)| p.name == *n))
            .collect();
        let (Some((first, _)), Some(group)) = (members.first(), BodyGroup::of(names[0])) else {
            continue;
        };
        let points: Vec<[f32; 3]> = members
            .iter()
            .flat_map(|(p, g)| {
                g.positions
                    .iter()
                    .map(|v| to_local(&first.cframe, to_parent(&p.cframe, *v)))
            })
            .collect();
        let Some(ext) = extent(&points) else {
            continue;
        };
        let (min, max) = spec::body_bounds(Some(group));
        if let Some(f) = check(Rule::BodyPartSize, asset, ext, min, max) {
            out.push(f);
        }
    }
    if item.asset_type == AssetType::DynamicHead {
        return;
    }
    let points: Vec<[f32; 3]> = usable
        .iter()
        .flat_map(|(p, g)| g.positions.iter().map(|v| to_parent(&p.cframe, *v)))
        .collect();
    if let Some(ext) = extent(&points) {
        let (min, max) = spec::body_bounds(None);
        if let Some(f) = check(Rule::BodySize, "Body", ext, min, max) {
            out.push(f);
        }
    }
}

fn check(
    rule: Rule,
    subject: &str,
    ext: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
) -> Option<Finding> {
    let (limit, side) = if (0..3).any(|i| ext[i] > max[i] + SLACK) {
        (max, "larger than the most")
    } else if (0..3).any(|i| ext[i] < min[i] - SLACK) {
        (min, "smaller than the least")
    } else {
        return None;
    };
    let [w, h, d] = limit;
    Some(Finding {
        rule,
        severity: Severity::Error,
        subject: subject.to_string(),
        measured: Some(Value::Studs(ext)),
        limit: Some(Value::Studs(limit)),
        message: format!(
            "The {subject} is {side} Roblox allows, {w:.2} \u{d7} {h:.2} \u{d7} {d:.2} studs (width \u{d7} length \u{d7} depth); resize it to fit."
        ),
    })
}

#[cfg(test)]
#[path = "size/tests.rs"]
mod tests;
