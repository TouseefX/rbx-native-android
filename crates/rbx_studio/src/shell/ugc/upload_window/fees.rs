//! The Fees card: what this upload costs and what the Creator Hub step will.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use rbx_ugc::spec;

use super::super::ItemKind;
use crate::launcher::ui::{self, Weight};
use crate::tokens;

const FEES_PAGE: &str =
    "https://create.roblox.com/docs/marketplace/marketplace-fees-and-commissions";

/// `n` with comma thousands separators.
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn upload_fee(emissive: bool) -> u32 {
    if emissive {
        spec::EMISSIVE_UPLOAD_FEE
    } else {
        spec::UPLOAD_FEE
    }
}

fn advance(kind: ItemKind, emissive: bool) -> String {
    match spec::publishing_advance(kind.asset) {
        Some((plain, limited)) => {
            let plain = if emissive {
                plain * spec::EMISSIVE_ADVANCE_FACTOR
            } else {
                plain
            };
            format!(
                "{} non-limited \u{b7} {} paid limited",
                thousands(plain),
                thousands(limited)
            )
        }
        None => format!("The fees page lists no advance for {}", kind.asset.label()),
    }
}

fn row(title: &'static str, detail: String, amount: String) -> Div {
    h_flex()
        .items_start()
        .gap(px(12.))
        .py(px(8.))
        .border_b_1()
        .border_color(tokens::border())
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(
                    ui::text(13., 18.)
                        .text_color(tokens::text_strong())
                        .child(title),
                )
                .child(ui::text(12., 16.).text_color(tokens::text2()).child(detail)),
        )
        .child(
            ui::text(13., 18.)
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens::text_strong())
                .text_right()
                .child(amount),
        )
}

pub(super) fn fees_view(kind: ItemKind, emissive: Option<bool>) -> AnyElement {
    let is_emissive = emissive.unwrap_or(false);
    let mask = match emissive {
        Some(true) => "This item has an emissive mask, so the higher fee applies",
        Some(false) => "This item has no emissive mask",
        None => "Whether this item has an emissive mask is known once it is checked",
    };
    v_flex()
        .gap(px(12.))
        .child(
            v_flex()
                .child(row(
                    "Upload from rbx-native",
                    "Open Cloud doesn\u{2019}t charge for a Model upload.".to_string(),
                    "Free".to_string(),
                ))
                .child(row(
                    "Upload fee on the Creator Hub",
                    format!("Per submission, not refunded if moderation rejects it. {mask}."),
                    format!("{} Robux", thousands(upload_fee(is_emissive))),
                ))
                .child(row(
                    "Publishing advance",
                    "Refundable, recouped from your sales.".to_string(),
                    advance(kind, is_emissive),
                )),
        )
        .child(
            v_flex()
                .gap(px(8.))
                .p(px(12.))
                .rounded(px(8.))
                .bg(ui::wash())
                .child(ui::text(12.5, 18.).text_color(tokens::text()).child(
                    "Roblox has no public API for submitting avatar items, so this uploads your item as a Model and opens it on the Creator Hub. Nothing is paid from here.",
                ))
                .child(ui::text(12.5, 18.).text_color(tokens::text2()).child(
                    "Not confirmed yet: whether the Creator Hub can turn a Model uploaded this way into an avatar item. If it can\u{2019}t, open the item in Roblox Studio and use Save to Roblox \u{203a} Avatar Item.",
                ))
                .child(
                    h_flex().child(
                        ui::external_button("ugc-upload-fees-link", "Fees and commissions", Weight::Ghost, true)
                            .on_click(|_, _, cx| cx.open_url(FEES_PAGE)),
                    ),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn thousands_are_separated_by_commas() {
        assert_eq!(thousands(80), "80");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
