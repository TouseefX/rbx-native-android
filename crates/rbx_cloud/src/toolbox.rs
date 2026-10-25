//! Open Cloud's Creator Store API (creator-docs
//! `reference/cloud/toolbox-service/v1.json`): search and details for
//! the assets in the Creator Store, the only keyed route to them. Needs an
//! API key with the `creator-store-product:read` scope.

use serde::Deserialize;

use crate::client::Client;
use crate::error::{self, CloudError};
use crate::parallel::fan_out;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreCategory {
    Audio,
    Model,
    Decal,
    Plugin,
    MeshPart,
    Video,
    FontFamily,
}

impl StoreCategory {
    const ALL: [StoreCategory; 7] = [
        StoreCategory::Audio,
        StoreCategory::Model,
        StoreCategory::Decal,
        StoreCategory::Plugin,
        StoreCategory::MeshPart,
        StoreCategory::Video,
        StoreCategory::FontFamily,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StoreCategory::Audio => "Audio",
            StoreCategory::Model => "Model",
            StoreCategory::Decal => "Decal",
            StoreCategory::Plugin => "Plugin",
            StoreCategory::MeshPart => "MeshPart",
            StoreCategory::Video => "Video",
            StoreCategory::FontFamily => "FontFamily",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str().eq_ignore_ascii_case(text.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreCreator {
    User(u64),
    Group(u64),
}

#[derive(Debug, Clone, Default)]
pub struct StoreSearch {
    pub query: String,
    /// `None` searches models.
    pub category: Option<StoreCategory>,
    pub creator: Option<StoreCreator>,
    pub min_price_cents: Option<u64>,
    pub max_price_cents: Option<u64>,
    pub verified_only: bool,
    pub model_sub_types: Vec<String>,
    /// More keywords that narrow the search, as the store suggests them.
    pub facets: Vec<String>,
    /// Audio only.
    pub audio_min_seconds: Option<u32>,
    pub audio_max_seconds: Option<u32>,
    /// Clamped to 1..=100; 0 means 25.
    pub page_size: u32,
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoreAsset {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub asset_type_id: u32,
    pub creator_name: String,
    pub creator: Option<StoreCreator>,
    pub verified: bool,
    pub purchasable: bool,
    /// The Creator Store sells for real money, so this is the
    /// `currency`'s cents; `None` when the answer carries no price (a free
    /// item answers 0).
    pub price_cents: Option<u64>,
    /// ISO 4217, such as `USD`; empty when the answer names none.
    pub currency: String,
    pub up_vote_percent: Option<u32>,
    pub vote_count: u64,
    pub update_time: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StorePage {
    pub assets: Vec<StoreAsset>,
    pub total_results: Option<u64>,
    pub next_page_token: Option<String>,
    /// Keywords the store suggests to narrow this search further.
    pub available_facets: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PageRaw {
    #[serde(rename = "creatorStoreAssets")]
    assets: Vec<EntryRaw>,
    #[serde(rename = "totalResults")]
    total_results: Option<u64>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "queryFacets")]
    facets: Option<FacetsRaw>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FacetsRaw {
    #[serde(rename = "availableFacets")]
    available: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EntryRaw {
    asset: AssetRaw,
    creator: CreatorRaw,
    #[serde(rename = "creatorStoreProduct")]
    product: ProductRaw,
    voting: VotingRaw,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AssetRaw {
    id: Option<u64>,
    name: String,
    description: String,
    #[serde(rename = "assetTypeId")]
    asset_type_id: u32,
    #[serde(rename = "updateTime")]
    update_time: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CreatorRaw {
    creator: String,
    #[serde(rename = "userId")]
    user_id: u64,
    #[serde(rename = "groupId")]
    group_id: u64,
    name: String,
    verified: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ProductRaw {
    purchasable: bool,
    #[serde(rename = "purchasePrice")]
    price: Option<PriceRaw>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PriceRaw {
    #[serde(rename = "currencyCode")]
    currency: String,
    quantity: QuantityRaw,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct QuantityRaw {
    significand: i64,
    exponent: i32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct VotingRaw {
    #[serde(rename = "voteCount")]
    vote_count: u64,
    #[serde(rename = "upVotePercent")]
    up_vote_percent: Option<u32>,
}

impl Client {
    /// `GET /toolbox-service/v2/assets:search`.
    pub fn search_creator_store(&self, search: &StoreSearch) -> Result<StorePage, CloudError> {
        let url = self.apis_url("/toolbox-service/v2/assets:search");
        let category = search.category.unwrap_or(StoreCategory::Model);
        let size = if search.page_size == 0 {
            25
        } else {
            search.page_size.clamp(1, 100)
        };
        let mut params: Vec<(&str, String)> = vec![
            ("searchCategoryType", category.as_str().to_string()),
            (
                "includeOnlyVerifiedCreators",
                search.verified_only.to_string(),
            ),
            ("maxPageSize", size.to_string()),
            ("searchView", "Full".to_string()),
        ];
        if !search.query.is_empty() {
            params.push(("query", search.query.clone()));
        }
        match search.creator {
            Some(StoreCreator::User(id)) => params.push(("creator", format!("user/{id}"))),
            Some(StoreCreator::Group(id)) => params.push(("creator", format!("group/{id}"))),
            None => {}
        }
        if let Some(cents) = search.min_price_cents {
            params.push(("minPriceCents", cents.to_string()));
        }
        if let Some(cents) = search.max_price_cents {
            params.push(("maxPriceCents", cents.to_string()));
        }
        if !search.model_sub_types.is_empty() {
            params.push(("modelSubTypes", search.model_sub_types.join(",")));
        }
        if let Some(seconds) = search.audio_min_seconds {
            params.push(("audioMinDurationSeconds", seconds.to_string()));
        }
        if let Some(seconds) = search.audio_max_seconds {
            params.push(("audioMaxDurationSeconds", seconds.to_string()));
        }
        for facet in &search.facets {
            params.push(("facets", facet.clone()));
        }
        if let Some(token) = &search.page_token {
            params.push(("pageToken", token.clone()));
        }
        let response = self.get_with(&url, true, true, |req| {
            params.iter().fold(req, |req, (k, v)| req.query(k, v))
        })?;
        if !(200..300).contains(&response.status) {
            return Err(error::error_for_status(
                &url,
                response.status,
                &response.headers,
            ));
        }
        parse_page(&response.body)
    }

    /// `GET /toolbox-service/v2/assets/{id}`.
    pub fn creator_store_asset(&self, id: u64) -> Result<StoreAsset, CloudError> {
        let url = self.apis_url(&format!("/toolbox-service/v2/assets/{id}"));
        let response = self.get_raw(&url, true, true)?;
        if !(200..300).contains(&response.status) {
            return Err(error::error_for_status(
                &url,
                response.status,
                &response.headers,
            ));
        }
        let raw: EntryRaw = serde_json::from_slice(&response.body)?;
        parse_asset(raw).ok_or_else(|| {
            CloudError::UnexpectedShape(format!("creator store asset {id} carried no asset id"))
        })
    }

    /// The details of every id, fetched in parallel, in `ids` order.
    pub fn creator_store_assets(&self, ids: &[u64]) -> Vec<Result<StoreAsset, CloudError>> {
        fan_out(ids, |&id| self.creator_store_asset(id))
    }
}

fn parse_page(body: &[u8]) -> Result<StorePage, CloudError> {
    let page: PageRaw = serde_json::from_slice(body)?;
    Ok(StorePage {
        assets: page.assets.into_iter().filter_map(parse_asset).collect(),
        total_results: page.total_results,
        next_page_token: page.next_page_token.filter(|t| !t.is_empty()),
        available_facets: page.facets.and_then(|f| f.available).unwrap_or_default(),
    })
}

fn parse_asset(raw: EntryRaw) -> Option<StoreAsset> {
    let id = raw.asset.id?;
    let creator = &raw.creator;
    let store_creator = if creator.user_id > 0 {
        Some(StoreCreator::User(creator.user_id))
    } else if creator.group_id > 0 {
        Some(StoreCreator::Group(creator.group_id))
    } else if let Some(n) = creator.creator.strip_prefix("users/") {
        n.parse().ok().map(StoreCreator::User)
    } else if let Some(n) = creator.creator.strip_prefix("groups/") {
        n.parse().ok().map(StoreCreator::Group)
    } else {
        None
    };
    let (price_cents, currency) = match raw.product.price {
        Some(PriceRaw { currency, quantity }) => {
            let cents = quantity.significand as f64 * 10f64.powi(quantity.exponent + 2);
            (Some(cents.round().max(0.0) as u64), currency)
        }
        None => (None, String::new()),
    };
    Some(StoreAsset {
        id,
        name: raw.asset.name,
        description: raw.asset.description,
        asset_type_id: raw.asset.asset_type_id,
        creator_name: raw.creator.name,
        creator: store_creator,
        verified: raw.creator.verified,
        purchasable: raw.product.purchasable,
        price_cents,
        currency,
        up_vote_percent: raw.voting.up_vote_percent,
        vote_count: raw.voting.vote_count,
        update_time: raw.asset.update_time,
    })
}

#[cfg(test)]
mod tests;
