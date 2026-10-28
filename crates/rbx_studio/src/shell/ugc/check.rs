//! The check both windows run: what is checked, as what, and its result,
//! re-run in the background whenever the source or the kind changes.

use gpui_kit::*;
use rbx_dom::Variant;
use rbx_ugc::{AssetType, BodyScale, Finding, Geometry};

use super::super::Shell;
use super::{collect, source, Collected, ItemKind, ItemSource};

pub(super) struct Done {
    pub(super) collected: Collected,
    pub(super) findings: Vec<Finding>,
}

pub(super) enum Run {
    Idle,
    Running,
    Done(Done),
    Failed(String),
}

pub(super) struct Check {
    pub(super) source: Option<ItemSource>,
    pub(super) kind: ItemKind,
    pub(super) run: Run,
    /// The body cage a layered item's cages were made from, when this
    /// editor made them: both must keep its vertices and UVs.
    pub(super) template: Option<Geometry>,
    /// Bumped by every start, so an older run finishing late is dropped.
    generation: u64,
}

fn squash(label: &str) -> String {
    label
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '&'))
        .collect()
}

impl Check {
    pub(super) fn new(source: Option<ItemSource>, kind: ItemKind) -> Self {
        Check {
            source,
            kind,
            run: Run::Idle,
            template: None,
            generation: 0,
        }
    }

    /// The single selected instance as a source, when it is an `Accessory`,
    /// a `BasePart` or a `Model`.
    pub(super) fn selection(shell: &Shell) -> Option<ItemSource> {
        let root = shell.selected()?;
        let inst = shell.dom.get(root)?;
        let class = inst.class();
        let fits = class == "Accessory"
            || class == "Model"
            || shell.database.is_subclass_of(class, "BasePart");
        fits.then(|| ItemSource::Instance {
            root,
            name: inst.name().to_string(),
            class: class.to_string(),
        })
    }

    /// The kind an instance most likely is: a `Model` holding `Head` and
    /// `UpperTorso` is a Body, a head with a `FaceControls` (alone or as
    /// a `Model`'s only body part) a Head, an `Accessory` by its
    /// `AccessoryType` (read through the reflection database's enum), else
    /// `None`.
    pub(super) fn guess_kind(shell: &Shell, source: &ItemSource) -> Option<ItemKind> {
        let ItemSource::Instance { root, class, .. } = source else {
            return None;
        };
        let inst = shell.dom.get(*root)?;
        let normal = |asset| ItemKind {
            asset,
            scale: BodyScale::Normal,
        };
        let has_face = |part: Option<rbx_dom::Ref>| {
            part.and_then(|p| shell.dom.get(p)).is_some_and(|p| {
                p.children()
                    .iter()
                    .filter_map(|c| shell.dom.get(*c))
                    .any(|c| c.class() == "FaceControls")
            })
        };
        match class.as_str() {
            "MeshPart" if has_face(Some(*root)) => Some(normal(AssetType::DynamicHead)),
            "Model" => {
                let has = |name: &str| {
                    inst.children()
                        .iter()
                        .filter_map(|c| shell.dom.get(*c))
                        .any(|c| c.name() == name)
                };
                if has("Head") && has("UpperTorso") {
                    Some(normal(AssetType::Body))
                } else {
                    has_face(collect::head_part(&shell.dom, *root))
                        .then(|| normal(AssetType::DynamicHead))
                }
            }
            "Accessory" => {
                let Some(Variant::Enum(v)) = inst.properties().get("AccessoryType") else {
                    return None;
                };
                let name: String = shell.database.enum_name("AccessoryType", *v)?.to_string();
                AssetType::ALL
                    .iter()
                    .find(|a| squash(a.label()) == name)
                    .map(|a| normal(*a))
            }
            _ => None,
        }
    }

    /// Reads and checks the source again off the UI thread. `slot` finds
    /// this `Check` in the window's view when the run comes back.
    pub(super) fn start<V: 'static>(
        &mut self,
        shell: &Entity<Shell>,
        cx: &mut Context<V>,
        slot: fn(&mut V) -> &mut Check,
    ) {
        self.generation += 1;
        let generation = self.generation;
        self.run = Run::Running;
        cx.notify();
        let Some(source) = self.source.clone() else {
            self.run = Run::Idle;
            return;
        };
        let kind = self.kind;

        let work: Box<dyn FnOnce() -> Result<Collected, String> + Send> = match source {
            ItemSource::Instance { root, .. } => {
                let s = shell.read(cx);
                let head = (kind.asset == AssetType::DynamicHead)
                    .then(|| collect::head_plan(&s.dom, &s.database, root))
                    .flatten();
                match collect::plan(&s.dom, root, kind) {
                    Ok(plan) => {
                        let template = self.template.clone();
                        Box::new(move || {
                            let mut collected = collect::resolve(&plan, &collect::fetch_cached);
                            for cage in &mut collected.item.cages {
                                cage.template = cage.template.take().or(template.clone());
                            }
                            collected.head = head.map(|head| {
                                Box::new(collect::resolve_head(&head, &collect::fetch_cached).0)
                            });
                            Ok(collected)
                        })
                    }
                    Err(msg) => {
                        self.run = Run::Failed(msg);
                        return;
                    }
                }
            }
            ItemSource::File(path) => Box::new(move || source::item_from_file(&path, kind)),
        };

        let task = cx.background_spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |view, cx| {
                let check = slot(view);
                if check.generation != generation {
                    return;
                }
                check.run = match result {
                    Ok(collected) => Run::Done(Done {
                        findings: collected.findings(),
                        collected,
                    }),
                    Err(msg) => Run::Failed(msg),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A finished run with at least one error.
    pub(super) fn blocking(&self) -> bool {
        matches!(&self.run, Run::Done(done) if rbx_ugc::blocks(&done.findings))
    }
}
