//! The item being fitted: a `MeshPart`, or the biggest `MeshPart` of a
//! `Model`, which becomes the handle.

use gpui_kit::*;
use rbx_dom::{Ref, Variant, WeakDom};

use super::preview;
use super::state::{AutoSkin, Mode, Wrap};
use super::{FittingWindow, Job};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Source {
    /// The `MeshPart` that becomes the handle.
    pub(super) part: Ref,
    /// Its name, which the accessory takes.
    pub(super) name: String,
    /// What was picked, when that was a `Model`, and how the handle was
    /// chosen from it.
    pub(super) note: Option<String>,
}

fn mesh_parts(dom: &WeakDom, root: Ref, out: &mut Vec<Ref>) {
    let Some(inst) = dom.get(root) else { return };
    for &c in inst.children() {
        if dom.get(c).is_some_and(|i| i.class() == "MeshPart") {
            out.push(c);
        }
        mesh_parts(dom, c, out);
    }
}

/// The item `root` stands for, or why it cannot be one.
pub(super) fn resolve(dom: &WeakDom, root: Ref) -> Result<Source, String> {
    let inst = dom.get(root).ok_or("That item is no longer in the place")?;
    let named = |part: Ref| {
        dom.get(part)
            .map_or_else(String::new, |p| p.name().to_string())
    };
    match inst.class() {
        "MeshPart" => Ok(Source {
            part: root,
            name: inst.name().to_string(),
            note: None,
        }),
        "Model" | "Folder" => {
            let mut parts = Vec::new();
            mesh_parts(dom, root, &mut parts);
            let volume = |p: &Ref| preview::size(dom, *p).iter().product::<f32>();
            let part = parts
                .iter()
                .copied()
                .max_by(|a, b| volume(a).total_cmp(&volume(b)))
                .ok_or_else(|| format!("{} has no MeshPart to fit", inst.name()))?;
            let name = named(part);
            let note = if parts.len() == 1 {
                format!(
                    "{name}, the only MeshPart in {}, is the handle.",
                    inst.name()
                )
            } else {
                format!(
                    "{name}, the biggest of the {} MeshParts in {}, is the handle; the others stay out of the accessory.",
                    parts.len(),
                    inst.name()
                )
            };
            Ok(Source {
                part,
                name,
                note: Some(note),
            })
        }
        other => Err(format!(
            "{} is a {other}. Pick a MeshPart, or a Model holding one.",
            inst.name()
        )),
    }
}

/// The `WrapLayer` settings a part already carries, if it has one: a
/// catalog item being refitted keeps its own.
pub(super) fn wrap_of(dom: &WeakDom, part: Ref) -> Option<Wrap> {
    let layer = dom
        .get(part)?
        .children()
        .iter()
        .copied()
        .find(|&c| dom.get(c).is_some_and(|i| i.class() == "WrapLayer"))?;
    let props = dom.get(layer)?.properties();
    let float = |key: &str, or: f32| match props.get(key) {
        Some(Variant::Float32(v)) => *v,
        _ => or,
    };
    let base = Wrap::default();
    Some(Wrap {
        order: match props.get("Order") {
            Some(Variant::Int32(v)) => *v,
            _ => base.order,
        },
        puffiness: float("Puffiness", base.puffiness),
        shrink: float("ShrinkFactor", base.shrink),
        auto_skin: match props.get("AutoSkin") {
            Some(Variant::Enum(v)) => AutoSkin::from_value(*v),
            _ => base.auto_skin,
        },
    })
}

impl FittingWindow {
    /// Makes `root` the item, or says why it cannot be.
    pub(super) fn pick(&mut self, root: Ref, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, wrap) = {
            let dom = &self.shell.read(cx).dom;
            let picked = resolve(dom, root);
            let wrap = picked.as_ref().ok().and_then(|s| wrap_of(dom, s.part));
            (picked, wrap)
        };
        match picked {
            Ok(source) => {
                if let Some(wrap) = wrap {
                    self.settings.set_mode(Mode::Clothing);
                    self.settings.wrap = wrap;
                }
                self.source = Some(source);
                self.source_error = None;
                self.generated = None;
                self.cages = Job::Idle;
                self.inputs.show_settings(&self.settings, window, cx);
            }
            Err(message) => self.source_error = Some(message),
        }
        cx.notify();
    }

    pub(super) fn use_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.shell.read(cx).selected() else {
            self.source_error =
                Some("Nothing is selected. Select a MeshPart or a Model in the place.".into());
            cx.notify();
            return;
        };
        self.pick(root, window, cx);
        self.sync(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve, wrap_of, AutoSkin, Ref, Variant, WeakDom};

    fn sized(dom: &mut WeakDom, name: &str, parent: Ref, edge: f32) -> Ref {
        let part = dom.new_instance("MeshPart", name, Some(parent));
        dom.set_property(
            part,
            "size",
            Variant::Vector3(rbx_dom::Vector3Data {
                x: edge,
                y: edge,
                z: edge,
            }),
        )
        .unwrap();
        part
    }

    #[test]
    fn a_model_uses_its_biggest_mesh_part_and_says_so() {
        let mut dom = WeakDom::new();
        let model = dom.new_instance("Model", "Crown", None);
        sized(&mut dom, "Gem", model, 0.5);
        let band = sized(&mut dom, "Band", model, 2.0);
        let source = resolve(&dom, model).unwrap();
        assert_eq!(source.part, band);
        assert_eq!(source.name, "Band");
        assert!(source.note.unwrap().contains("biggest of the 2"));
    }

    #[test]
    fn a_mesh_part_is_its_own_handle_and_anything_else_is_refused() {
        let mut dom = WeakDom::new();
        let ws = dom.new_instance("Workspace", "Workspace", None);
        let bow = sized(&mut dom, "Bow", ws, 1.0);
        assert_eq!(resolve(&dom, bow).unwrap().note, None);
        let script = dom.new_instance("Script", "S", Some(ws));
        assert!(resolve(&dom, script).unwrap_err().contains("Script"));
        let empty = dom.new_instance("Model", "Empty", Some(ws));
        assert!(resolve(&dom, empty).is_err());
    }

    #[test]
    fn an_existing_wrap_layer_is_kept() {
        let mut dom = WeakDom::new();
        let part = dom.new_instance("MeshPart", "Jacket", None);
        let layer = dom.new_instance("WrapLayer", "WrapLayer", Some(part));
        dom.set_property(layer, "Order", Variant::Int32(4)).unwrap();
        dom.set_property(layer, "AutoSkin", Variant::Enum(2))
            .unwrap();
        let wrap = wrap_of(&dom, part).unwrap();
        assert_eq!(wrap.order, 4);
        assert_eq!(wrap.auto_skin, AutoSkin::EnabledOverride);
        assert_eq!(wrap.puffiness, 1.0);
    }
}
