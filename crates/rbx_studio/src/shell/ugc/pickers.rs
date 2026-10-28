//! Controls both windows share: the source card and the asset type and body
//! scale pickers.

use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rbx_ugc::{AssetType, BodyScale, Category};

use super::{ItemKind, ItemSource};
use crate::launcher::ui::{self, Weight};
use crate::tokens;

/// What is being checked, with Use Selection and Choose File buttons.
pub(super) fn source_card(
    id: &'static str,
    source: Option<&ItemSource>,
    on_selection: impl Fn(&mut Window, &mut App) + 'static,
    on_file: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let (glyph, title, detail) = match source {
        Some(ItemSource::Instance { class, .. }) => (
            if class == "Accessory" { "shirt" } else { "box" },
            source.map(ItemSource::label).unwrap_or_default(),
            class.clone(),
        ),
        Some(file @ ItemSource::File(path)) => {
            ("file-box", file.label(), path.display().to_string())
        }
        None => (
            "scan-search",
            "Nothing picked".to_string(),
            "Select an Accessory, MeshPart or Model, or choose a .fbx, .obj, .gltf or .glb."
                .to_string(),
        ),
    };
    h_flex()
        .gap(px(12.))
        .child(
            div()
                .flex_none()
                .size(px(36.))
                .rounded(px(8.))
                .bg(tokens::accent_soft())
                .flex()
                .items_center()
                .justify_center()
                .child(ui::icon(glyph, 18.).text_color(ui::accent())),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(
                    ui::text(14., 20.)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens::text_strong())
                        .truncate()
                        .child(title),
                )
                .child(
                    ui::text(12., 16.)
                        .text_color(tokens::text2())
                        .truncate()
                        .child(detail),
                ),
        )
        .child(
            ui::icon_button(
                SharedString::from(format!("{id}-selection")),
                "mouse-pointer-click",
                "Use Selection",
                Weight::Secondary,
                true,
            )
            .flex_none()
            .on_click(move |_, window, cx| on_selection(window, cx)),
        )
        .child(
            ui::icon_button(
                SharedString::from(format!("{id}-file")),
                "folder-open",
                "Choose File\u{2026}",
                Weight::Secondary,
                true,
            )
            .flex_none()
            .on_click(move |_, window, cx| on_file(window, cx)),
        )
        .into_any_element()
}

fn label_row(label: &'static str, body: impl IntoElement) -> Div {
    h_flex()
        .items_start()
        .gap(px(12.))
        .child(
            div()
                .flex_none()
                .w(px(84.))
                .pt(px(4.))
                .child(ui::text(12., 16.).text_color(tokens::text2()).child(label)),
        )
        .child(div().flex_1().min_w_0().child(body))
}

/// A click handler shared by every pill of a row.
type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

fn pill(
    id: SharedString,
    label: &'static str,
    accented: bool,
    disabled: bool,
    on_click: OnClick,
) -> AnyElement {
    let base = ui::pill(label, None, accented).id(id);
    if disabled {
        return base.opacity(0.5).into_any_element();
    }
    base.cursor_pointer()
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}

/// Asset type pills grouped Rigid, Layered and Body, then the body scale
/// pills when the type is rigid. `on_pick` gets the new kind.
pub(super) fn kind_picker(
    id: &'static str,
    kind: ItemKind,
    disabled: bool,
    on_pick: impl Fn(ItemKind, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let groups = [
        (Category::Rigid, "Rigid"),
        (Category::Layered, "Layered"),
        (Category::Body, "Body"),
    ];
    let mut types = v_flex().gap(px(8.));
    for (category, caption) in groups {
        let pills = AssetType::ALL
            .iter()
            .filter(|a| a.category() == category)
            .map(|asset| {
                let asset = *asset;
                let on_pick = on_pick.clone();
                pill(
                    SharedString::from(format!("{id}-type-{}", asset.label())),
                    asset.label(),
                    asset == kind.asset,
                    disabled,
                    Rc::new(move |window, cx| on_pick(ItemKind { asset, ..kind }, window, cx)),
                )
            });
        types = types.child(
            v_flex()
                .gap(px(4.))
                .child(
                    ui::text(11., 14.)
                        .text_color(tokens::text3())
                        .child(caption),
                )
                .child(h_flex().flex_wrap().gap(px(6.)).children(pills)),
        );
    }
    let rigid = kind.asset.category() == Category::Rigid;
    v_flex()
        .gap(px(12.))
        .child(label_row("Asset type", types))
        .when(rigid, |el| {
            let pills = BodyScale::ALL.iter().map(|scale| {
                let scale = *scale;
                let on_pick = on_pick.clone();
                pill(
                    SharedString::from(format!("{id}-scale-{}", scale.label())),
                    scale.label(),
                    scale == kind.scale,
                    disabled,
                    Rc::new(move |window, cx| on_pick(ItemKind { scale, ..kind }, window, cx)),
                )
            });
            el.child(label_row(
                "Body scale",
                h_flex().flex_wrap().gap(px(6.)).children(pills),
            ))
        })
        .into_any_element()
}
