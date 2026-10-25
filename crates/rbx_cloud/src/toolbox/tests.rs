use super::*;

const PAGE: &str = r#"{"creatorStoreAssets":[
{"asset":{"id":11,"name":"Tree","description":"A tree","assetTypeId":10,"updateTime":"2026-01-01T00:00:00Z"},
 "creator":{"creator":"users/5","userId":5,"name":"Ann","verified":true},
 "creatorStoreProduct":{"purchasable":false,"purchasePrice":{"currencyCode":"USD","quantity":{"significand":0,"exponent":0}}},
 "voting":{"upVotes":9,"downVotes":1,"voteCount":10,"upVotePercent":90}},
{"asset":{"id":12,"name":"Sticker","assetTypeId":13},
 "creator":{"creator":"groups/2","groupId":2,"name":"Team"},
 "creatorStoreProduct":{"purchasable":true,"purchasePrice":{"quantity":{"significand":125,"exponent":0}}}},
{"asset":{"id":13,"name":"Pricey"},
 "creator":{"creator":"users/8"},
 "creatorStoreProduct":{"purchasePrice":{"quantity":{"significand":9900,"exponent":-2}}}},
{"asset":{"name":"no id"}}
],"totalResults":42,"nextPageToken":"abc"}"#;

#[test]
fn parses_a_realistic_page() {
    let page = parse_page(PAGE.as_bytes()).unwrap();
    assert_eq!(page.total_results, Some(42));
    assert_eq!(page.next_page_token.as_deref(), Some("abc"));
    assert_eq!(page.assets.len(), 3);

    let tree = &page.assets[0];
    assert_eq!(tree.id, 11);
    assert_eq!(tree.creator, Some(StoreCreator::User(5)));
    assert_eq!(tree.price_cents, Some(0));
    assert_eq!(tree.currency, "USD");
    assert!(tree.verified && !tree.purchasable);
    assert_eq!(tree.up_vote_percent, Some(90));
    assert_eq!(tree.vote_count, 10);

    let sticker = &page.assets[1];
    assert_eq!(sticker.creator, Some(StoreCreator::Group(2)));
    assert_eq!(sticker.price_cents, Some(12500));
    assert_eq!(sticker.asset_type_id, 13);
    assert!(sticker.purchasable);

    let pricey = &page.assets[2];
    assert_eq!(pricey.price_cents, Some(9900));
    assert_eq!(pricey.creator, Some(StoreCreator::User(8)));
}

#[test]
fn missing_fields_and_empty_token_are_tolerated() {
    let page =
        parse_page(br#"{"creatorStoreAssets":[{"asset":{"id":1}}],"nextPageToken":""}"#).unwrap();
    assert_eq!(page.next_page_token, None);
    assert_eq!(page.total_results, None);
    let asset = &page.assets[0];
    assert_eq!(asset.price_cents, None);
    assert_eq!(asset.creator, None);
    assert_eq!(asset.up_vote_percent, None);
    assert!(asset.name.is_empty());
    assert!(parse_page(b"{}").unwrap().assets.is_empty());
}

#[test]
fn categories_parse_case_insensitively() {
    assert_eq!(
        StoreCategory::parse("meshpart"),
        Some(StoreCategory::MeshPart)
    );
    assert_eq!(StoreCategory::parse("AUDIO"), Some(StoreCategory::Audio));
    assert_eq!(StoreCategory::parse("nope"), None);
}

#[test]
fn search_sends_the_documented_query() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let seen = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        while !String::from_utf8_lossy(&request).contains("\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
        }
        let body = r#"{"creatorStoreAssets":[]}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&request).to_string()
    });
    let client = Client::new(Some(crate::ApiKey::new("k"))).with_apis_root(root);
    let page = client
        .search_creator_store(&StoreSearch {
            query: "tree".into(),
            category: Some(StoreCategory::Audio),
            creator: Some(StoreCreator::Group(7)),
            page_size: 500,
            ..StoreSearch::default()
        })
        .unwrap();
    assert!(page.assets.is_empty());
    let head = seen.join().unwrap();
    let line = head.lines().next().unwrap().to_string();
    assert!(
        line.contains("/toolbox-service/v2/assets:search?"),
        "{line}"
    );
    assert!(line.contains("searchCategoryType=Audio"), "{line}");
    assert!(
        line.contains("creator=group%2F7") || line.contains("creator=group/7"),
        "{line}"
    );
    assert!(line.contains("includeOnlyVerifiedCreators=false"), "{line}");
    assert!(line.contains("maxPageSize=100"), "{line}");
    assert!(head.to_lowercase().contains("x-api-key: k"), "{head}");
}

/// The calls `search_asset` and `insert_asset` make, against Roblox:
/// `RBX_API_KEY=… cargo test -p rbx_cloud live_asset_tools -- --ignored`.
/// The key needs `creator-store-product:read` and `user.inventory-item:read`.
#[test]
#[ignore]
fn live_asset_tools() {
    let Some(key) = crate::ApiKey::from_env_or_config() else {
        return eprintln!("RBX_API_KEY is not set; skipped");
    };
    let client = Client::new(Some(key));
    let search = StoreSearch {
        query: "crate".to_owned(),
        page_size: 3,
        ..StoreSearch::default()
    };
    let page = client.search_creator_store(&search).unwrap();
    assert!(!page.assets.is_empty());
    let first = &page.assets[0];
    assert_eq!(
        client.creator_store_asset(first.id).unwrap().name,
        first.name
    );

    let owner = client.introspect().unwrap().authorized_user_id;
    let inventory = client.inventory_assets(owner, &["MODEL"], 3, None).unwrap();
    eprintln!(
        "{} inventory models on the first page",
        inventory.assets.len()
    );
}
