//! The hidden surface removal data of a body's `WrapTarget`s: what lets the
//! viewer hide the body under layered clothing the way Studio does.
//!
//! Roblox ships it on every body part's `WrapTarget`, inline as `HSRData`
//! (with the `HSRMeshIdData` naming the mesh and cage it was made for) or as
//! an `HSRAssetId`. A player's package parts carry it, and a rig built from
//! them keeps it ([`of`]). The stock bodies are built from tables, so they
//! get the `HSRAssetId` the reference avatar models give each body mesh
//! ([`stock`]).

use rbx_dom::{Content, Instance, Variant};

/// The `WrapTarget` properties that carry the data.
const PROPERTIES: [&str; 3] = ["HSRAssetId", "HSRData", "HSRMeshIdData"];

/// Body mesh and the `HSRAssetId` of its `WrapTarget`, for every stock
/// part with a cage (`bundle::tables`), from the reference avatar models.
const STOCK: [(u64, u64); 44] = [
    (123480233606534, 110944204503586),
    (8650495385, 137923628894853),
    (8650495328, 115431739133056),
    (8650495273, 139384548866964),
    (8650495270, 124536614395110),
    (8650495301, 129334982122173),
    (8650495362, 130219349855783),
    (8650495364, 122897041060272),
    (8650495382, 116557315797963),
    (8650495303, 121730493758372),
    (8650495271, 124830741456884),
    (8650495280, 105715906537771),
    (8650495376, 123779046665253),
    (8650495379, 108551329553647),
    (8650495330, 109444827978758),
    (7783843605, 12946171122),
    (7783843553, 12946171120),
    (7783843549, 108080898489647),
    (7783843554, 107413284852472),
    (7783843526, 110720988402410),
    (7783843593, 79542312113029),
    (7783843598, 119896233281352),
    (7783843596, 109090734692181),
    (7783843521, 82470326997124),
    (7783843520, 128805290572141),
    (7783843543, 118861368347199),
    (7783843630, 105060334785014),
    (7783843631, 82665147875608),
    (7783843601, 108726604604068),
    (82622532780174, 94501098804885),
    (81333791777583, 73365551427144),
    (70696180344244, 100231335472912),
    (76056457876800, 99537903414350),
    (111354745759987, 130255148465824),
    (72614976717198, 82440860605106),
    (111711537553966, 137010077510373),
    (92453943837525, 102233743827800),
    (76344974163258, 95079759153965),
    (112831543729235, 104953582771058),
    (105886794388916, 125507384896274),
    (102725541264741, 82017361663835),
    (72947425138427, 129630379533572),
    (128061966911086, 110041318263204),
    (82008215934091, 84811226317033),
];

pub(super) type Props = Vec<(String, Variant)>;

/// The HSR properties `wrap` carries, empty when it has none.
pub(super) fn of(wrap: &Instance) -> Props {
    let properties = wrap.properties();
    let filled = |value: &Variant| match value {
        Variant::Unknown { raw, .. } => !raw.is_empty(),
        Variant::String(s) | Variant::Content(Content::Uri(s)) => !s.is_empty(),
        _ => false,
    };
    if !["HSRAssetId", "HSRData"]
        .iter()
        .any(|key| properties.get(*key).is_some_and(filled))
    {
        return Vec::new();
    }
    PROPERTIES
        .iter()
        .filter_map(|key| Some((key.to_string(), properties.get(*key)?.clone())))
        .collect()
}

/// The `HSRAssetId` of the stock body part drawn with `mesh`.
pub(super) fn stock(mesh: u64) -> Option<u64> {
    STOCK.iter().find(|(m, _)| *m == mesh).map(|(_, hsr)| *hsr)
}

#[cfg(test)]
#[path = "hidden_surfaces/tests.rs"]
mod tests;
