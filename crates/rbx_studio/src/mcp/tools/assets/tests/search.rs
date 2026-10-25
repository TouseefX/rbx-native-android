use serde_json::{json, Value};

use super::super::super::tests::{place, run};
use super::{json_body, mock, INTROSPECT};

/// Runs `search_asset` and reads its compact JSON answer.
fn search(arguments: Value) -> Value {
    let (error, text) = run(&place(), "search_asset", arguments);
    assert!(!error, "{text}");
    assert!(!text.contains('\n'), "Studio answers compact JSON: {text}");
    serde_json::from_str(&text).unwrap()
}

fn store_page() -> (u16, Vec<u8>) {
    json_body(json!({
        "totalResults": 120,
        "queryFacets": { "availableFacets": ["storm", "thunder"] },
        "creatorStoreAssets": [{
            "asset": { "id": 9, "name": "Rain", "assetTypeId": 3, "description": "Wet." },
            "creator": { "name": "Sounds Inc", "groupId": 7, "verified": true },
            "creatorStoreProduct": {
                "purchasable": true,
                "purchasePrice": { "currencyCode": "USD", "quantity": { "significand": 399, "exponent": -2 } },
            },
        }],
    }))
}

#[test]
fn a_creator_store_search_sends_the_filters_and_answers_studios_rows() {
    let seen = mock(|target, _| match target {
        t if t.contains("/api-keys/v1/introspect") => (200, INTROSPECT.as_bytes().to_vec()),
        t if t.contains("assets:search") => store_page(),
        _ => (404, Vec::new()),
    });
    let answer = search(json!({
        "query": "rain",
        "scope": "creator_store",
        "assetType": "Audio",
        "facets": ["storm"],
        "tags": ["Nature"],
        "priceFilter": "paid",
        "maxPriceCents": 500,
        "audioMaxDuration": 60,
        "maxResults": 3,
    }));
    assert_eq!(answer["status"], "success");
    assert_eq!(answer["scope"], "creator_store");
    assert_eq!(answer["resultCount"], 1);
    assert_eq!(answer["searchSources"], json!(["creator_store"]));
    assert_eq!(answer["availableFacets"], json!(["storm", "thunder"]));
    assert_eq!(
        answer["context"],
        json!({ "userId": "5", "isPublished": false })
    );
    assert_eq!(answer["nextSteps"].as_array().unwrap().len(), 4);
    let row = &answer["results"][0];
    assert_eq!(row["assetId"], "9");
    assert_eq!(row["assetType"], "Audio");
    assert_eq!(row["isFree"], false);
    assert_eq!(row["priceCents"], 399);
    assert_eq!(row["priceLabel"], "$3.99");
    assert_eq!(row["creatorName"], "Sounds Inc");
    assert_eq!(
        row["creatorStoreUrl"],
        "https://create.roblox.com/store/asset/9"
    );
    assert_eq!(row["source"], "creator_store");
    let target = seen
        .lock()
        .unwrap()
        .iter()
        .map(|(target, _)| target.clone())
        .find(|target| target.contains("assets:search"))
        .unwrap();
    for part in [
        "searchCategoryType=Audio",
        "query=rain",
        "minPriceCents=1",
        "maxPriceCents=500",
        "audioMaxDurationSeconds=60",
        "facets=storm",
        "facets=Nature",
        "maxPageSize=3",
        "includeOnlyVerifiedCreators=false",
    ] {
        assert!(target.contains(part), "{part} not in {target}");
    }
}

#[test]
fn an_inventory_search_names_items_and_matches_the_query() {
    mock(|target, _| match target {
        t if t.contains("/api-keys/v1/introspect") => (200, INTROSPECT.as_bytes().to_vec()),
        t if t.contains("/cloud/v2/users/5/inventory-items") => json_body(json!({
            "inventoryItems": [
                { "assetDetails": { "assetId": "11", "inventoryItemAssetType": "MODEL" } },
                { "assetDetails": { "assetId": "12", "inventoryItemAssetType": "PACKAGE" } },
                { "assetDetails": { "assetId": "13", "inventoryItemAssetType": "MODEL" } },
            ],
        })),
        t if t.contains("/toolbox-service/v2/assets/11") => json_body(json!({
            "asset": { "id": 11, "name": "Red Car", "assetTypeId": 10 },
            "creator": { "name": "Builder", "userId": 8 },
        })),
        t if t.contains("/assets/v1/assets/12") => json_body(json!({
            "assetId": "12", "assetType": "Model", "displayName": "Blue car",
            "creationContext": { "creator": { "userId": "5" } },
        })),
        t if t.contains("/assets/v1/assets/13") => {
            json_body(json!({ "assetId": "13", "assetType": "Model", "displayName": "Tree" }))
        }
        _ => (404, Vec::new()),
    });
    let answer = search(json!({ "scope": "user", "query": "CAR" }));
    assert_eq!(answer["searchSources"], json!(["user"]));
    let rows = answer["results"].as_array().unwrap();
    let names: Vec<&str> = rows
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Red Car", "Blue car"]);
    assert_eq!(rows[0]["creatorId"], "8");
    assert_eq!(rows[0]["creatorName"], "Builder");
    assert_eq!(rows[1]["assetType"], "Package");
    assert_eq!(rows[1]["creatorId"], "5");
    assert_eq!(rows[1]["source"], "inventory");
    assert_eq!(rows[1]["location"], "user");
}

#[test]
fn a_group_search_finds_what_the_group_published_and_says_so() {
    let seen = mock(|target, _| match target {
        t if t.contains("/api-keys/v1/introspect") => (200, INTROSPECT.as_bytes().to_vec()),
        t if t.contains("/legacy-develop/v1/user/groups/canmanage") => {
            json_body(json!({ "data": [{ "id": 7, "name": "Seven Studios" }] }))
        }
        t if t.contains("assets:search") => store_page(),
        _ => (404, Vec::new()),
    });
    let answer =
        search(json!({ "scope": "group", "groupName": "seven studios", "assetType": "Audio" }));
    assert_eq!(
        answer["groups"],
        json!([{ "id": 7, "name": "Seven Studios" }])
    );
    assert_eq!(answer["searchSources"], json!(["group"]));
    assert_eq!(answer["resultCount"], 1);
    let steps = answer["nextSteps"].to_string();
    assert!(steps.contains("no group inventory listing"), "{steps}");
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|(target, _)| target.contains("creator=group%2F7")
            || target.contains("creator=group/7")));

    let answer = search(json!({ "scope": "group" }));
    assert_eq!(answer["resultCount"], 0);
    assert!(answer["nextSteps"][0]
        .as_str()
        .unwrap()
        .contains("not owned by a group"));
}

#[test]
fn auto_skips_what_an_unpublished_place_has_not_got() {
    mock(|target, _| match target {
        t if t.contains("/api-keys/v1/introspect") => (200, INTROSPECT.as_bytes().to_vec()),
        t if t.contains("/cloud/v2/users/5/inventory-items") => {
            json_body(json!({ "inventoryItems": [] }))
        }
        t if t.contains("assets:search") => store_page(),
        _ => (404, Vec::new()),
    });
    let answer = search(json!({ "query": "rain", "assetType": "Audio" }));
    assert_eq!(answer["scope"], "auto");
    assert_eq!(
        answer["skippedSources"]["universe"],
        "game is not published"
    );
    assert_eq!(
        answer["skippedSources"]["group"],
        "this universe is not owned by a group"
    );
    assert_eq!(answer["searchSources"], json!(["user", "creator_store"]));
    assert_eq!(answer["resultCount"], 1);

    let answer = search(json!({ "query": "rain", "includeSources": ["creator_store"] }));
    assert!(answer.get("skippedSources").is_none());
    assert_eq!(answer["searchSources"], json!(["creator_store"]));
}

#[test]
fn a_search_it_cannot_run_answers_why_as_studio_does() {
    let answer = search(json!({ "scope": "universe" }));
    assert_eq!(answer["status"], "error");
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .contains("not yet published"));
    let answer = search(json!({ "scope": "user", "groupId": "123" }));
    assert_eq!(
        answer["error"],
        "groupId/groupName only apply when scope is \"group\"; scope is \"user\"."
    );
    let answer = search(json!({ "scope": "universe", "assetType": "Hat" }));
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .contains("not yet published"));
}
