//! `GET /cloud/v2/users/{id}/inventory-items` filtered to `CREATED_PLACE`:
//! every place a user created, private ones included, the only API-key
//! route to a user's private experiences on an unrestricted key. Needs
//! `user.inventory-item:read` on a key the user themself created.

use serde::Deserialize;

use crate::client::Client;
use crate::error::{self, CloudError};

#[derive(Deserialize)]
struct PageRaw {
    #[serde(rename = "inventoryItems", default)]
    items: Vec<ItemRaw>,
    #[serde(rename = "nextPageToken", default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct ItemRaw {
    #[serde(rename = "assetDetails")]
    asset_details: Option<AssetDetailsRaw>,
    #[serde(rename = "addTime", default)]
    add_time: String,
}

#[derive(Deserialize)]
struct AssetDetailsRaw {
    #[serde(rename = "assetId")]
    asset_id: String,
    #[serde(rename = "inventoryItemAssetType", default)]
    asset_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAsset {
    pub asset_id: u64,
    /// Open Cloud's name, such as MODEL or MESH_PART.
    pub asset_type: String,
    pub added: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryPage {
    pub assets: Vec<InventoryAsset>,
    pub next_page_token: Option<String>,
}

impl Client {
    /// One page of `user_id`'s inventory assets of the given Open Cloud
    /// types (all types when `asset_types` is empty).
    pub fn inventory_assets(
        &self,
        user_id: u64,
        asset_types: &[&str],
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<InventoryPage, CloudError> {
        let url = self.apis_url(&format!("/cloud/v2/users/{user_id}/inventory-items"));
        let types = if asset_types.is_empty() {
            "*".to_string()
        } else {
            asset_types.join(",")
        };
        let filter = format!("inventoryItemAssetTypes={types}");
        let size = page_size.clamp(1, 100).to_string();
        let response = self.get_with(&url, true, true, |req| {
            let req = req.query("maxPageSize", &size).query("filter", &filter);
            match page_token {
                Some(t) => req.query("pageToken", t),
                None => req,
            }
        })?;
        if !(200..300).contains(&response.status) {
            return Err(error::error_for_status(
                &url,
                response.status,
                &response.headers,
            ));
        }
        parse_assets(&response.body)
    }

    /// The ids of every place `user_id` created, oldest first, root places
    /// and sub-places alike.
    pub fn created_places(&self, user_id: u64) -> Result<Vec<u64>, CloudError> {
        let url = format!("https://apis.roblox.com/cloud/v2/users/{user_id}/inventory-items");
        let mut places = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let response = self.get_with(&url, true, true, |req| {
                let req = req
                    .query("maxPageSize", "100")
                    .query("filter", "inventoryItemAssetTypes=CREATED_PLACE");
                match &token {
                    Some(t) => req.query("pageToken", t),
                    None => req,
                }
            })?;
            if !(200..300).contains(&response.status) {
                return Err(error::error_for_status(
                    &url,
                    response.status,
                    &response.headers,
                ));
            }
            let (ids, next) = parse(&response.body)?;
            places.extend(ids);
            match next {
                Some(next) => token = Some(next),
                None => break,
            }
        }
        Ok(places)
    }
}

fn parse(body: &[u8]) -> Result<(Vec<u64>, Option<String>), CloudError> {
    let page: PageRaw = serde_json::from_slice(body)?;
    let ids = page
        .items
        .into_iter()
        .filter_map(|item| item.asset_details?.asset_id.parse().ok())
        .collect();
    Ok((ids, page.next_page_token.filter(|t| !t.is_empty())))
}

fn parse_assets(body: &[u8]) -> Result<InventoryPage, CloudError> {
    let page: PageRaw = serde_json::from_slice(body)?;
    let assets = page
        .items
        .into_iter()
        .filter_map(|item| {
            let details = item.asset_details?;
            Some(InventoryAsset {
                asset_id: details.asset_id.parse().ok()?,
                asset_type: details.asset_type,
                added: item.add_time,
            })
        })
        .collect();
    Ok(InventoryPage {
        assets,
        next_page_token: page.next_page_token.filter(|t| !t.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_assets_and_skips_other_items() {
        let body = br#"{"inventoryItems":[
{"assetDetails":{"assetId":"10","inventoryItemAssetType":"MODEL"},"addTime":"2025-01-01T00:00:00Z"},
{"badgeDetails":{"badgeId":"5"},"addTime":"2025-01-02T00:00:00Z"},
{"assetDetails":{"assetId":"11","inventoryItemAssetType":"DECAL"},"addTime":"2025-01-03T00:00:00Z"}],
"nextPageToken":"t"}"#;
        let page = parse_assets(body).unwrap();
        assert_eq!(
            page,
            InventoryPage {
                assets: vec![
                    InventoryAsset {
                        asset_id: 10,
                        asset_type: "MODEL".into(),
                        added: "2025-01-01T00:00:00Z".into()
                    },
                    InventoryAsset {
                        asset_id: 11,
                        asset_type: "DECAL".into(),
                        added: "2025-01-03T00:00:00Z".into()
                    },
                ],
                next_page_token: Some("t".into()),
            }
        );
    }

    #[test]
    fn parses_a_real_created_place_page() {
        // users/156, 2026-09-25, trimmed to one item.
        let body = br#"{"inventoryItems":[{"path":"users/156/inventory-items/VVNFUl9BU1NFVF9JRD0xNDU3OTY2","assetDetails":{"assetId":"1501","inventoryItemAssetType":"CREATED_PLACE","instanceId":"1457966"},"addTime":"2008-03-01T07:21:42.630Z"}],"nextPageToken":"djEv"}"#;
        assert_eq!(parse(body).unwrap(), (vec![1501], Some("djEv".to_string())));
        assert_eq!(
            parse(br#"{"inventoryItems":[],"nextPageToken":""}"#).unwrap(),
            (vec![], None)
        );
    }
}
