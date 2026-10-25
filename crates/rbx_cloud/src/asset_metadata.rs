//! `GET /assets/v1/assets/{id}`: an asset's type, name and description
//! through the Assets API, so callers can tell what an id is before fetching
//! its bytes. Needs the `asset:read` scope.

use serde::Deserialize;

use crate::client::Client;
use crate::error::{self, CloudError};
use crate::parallel::fan_out;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMetadata {
    pub asset_id: u64,
    /// Such as Model, Decal, Audio, Mesh, Image, Animation, Video.
    pub asset_type: String,
    pub display_name: String,
    pub description: String,
    /// Who it belongs to, from its `creationContext`.
    pub creator: Option<crate::StoreCreator>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MetadataRaw {
    #[serde(rename = "assetId")]
    asset_id: serde_json::Value,
    #[serde(rename = "assetType")]
    asset_type: String,
    #[serde(rename = "displayName")]
    display_name: String,
    description: String,
    #[serde(rename = "creationContext")]
    creation_context: CreationContextRaw,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CreationContextRaw {
    creator: CreatorRaw,
}

/// Ids arrive as strings in the Assets API's JSON.
#[derive(Deserialize, Default)]
#[serde(default)]
struct CreatorRaw {
    #[serde(rename = "userId")]
    user_id: serde_json::Value,
    #[serde(rename = "groupId")]
    group_id: serde_json::Value,
}

impl Client {
    pub fn asset_metadata(&self, asset_id: u64) -> Result<AssetMetadata, CloudError> {
        let url = self.apis_url(&format!("/assets/v1/assets/{asset_id}"));
        let response = self.get_raw(&url, true, true)?;
        if !(200..300).contains(&response.status) {
            return Err(error::error_for_status(
                &url,
                response.status,
                &response.headers,
            ));
        }
        parse(&response.body, asset_id)
    }

    /// The metadata of every id, a few requests at a time (the Assets API
    /// answers a burst with 429s), in `ids` order.
    pub fn asset_metadatas(&self, ids: &[u64]) -> Vec<Result<AssetMetadata, CloudError>> {
        fan_out(ids, |&id| self.asset_metadata(id))
    }
}

fn parse(body: &[u8], requested: u64) -> Result<AssetMetadata, CloudError> {
    let raw: MetadataRaw = serde_json::from_slice(body)?;
    let id = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => text.parse().ok(),
        other => other.as_u64(),
    };
    let asset_id = id(&raw.asset_id);
    let owner = &raw.creation_context.creator;
    let creator = match (id(&owner.user_id), id(&owner.group_id)) {
        (Some(user), _) => Some(crate::StoreCreator::User(user)),
        (None, Some(group)) => Some(crate::StoreCreator::Group(group)),
        _ => None,
    };
    Ok(AssetMetadata {
        asset_id: asset_id.unwrap_or(requested),
        asset_type: normalize_type(&raw.asset_type),
        display_name: raw.display_name,
        description: raw.description,
        creator,
    })
}

/// `ASSET_TYPE_MESH_PART` becomes `MeshPart`; an already mixed-case name
/// is left alone.
fn normalize_type(name: &str) -> String {
    let rest = name.strip_prefix("ASSET_TYPE_").unwrap_or(name);
    if rest.chars().any(|c| c.is_lowercase()) {
        return rest.to_string();
    }
    rest.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_asset_type_names() {
        assert_eq!(normalize_type("ASSET_TYPE_MESH_PART"), "MeshPart");
        assert_eq!(normalize_type("ASSET_TYPE_DECAL"), "Decal");
        assert_eq!(normalize_type("Decal"), "Decal");
        assert_eq!(normalize_type("MeshPart"), "MeshPart");
    }

    #[test]
    fn accepts_a_string_or_numeric_id() {
        let text = parse(br#"{"assetId":"42","assetType":"Model"}"#, 1).unwrap();
        let number = parse(br#"{"assetId":42,"displayName":"x"}"#, 1).unwrap();
        assert_eq!(text.asset_id, 42);
        assert_eq!(number.asset_id, 42);
        assert_eq!(number.display_name, "x");
    }

    #[test]
    fn fetches_metadata_with_the_key() {
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
            let body = r#"{"assetId":"42","assetType":"ASSET_TYPE_MESH_PART","displayName":"Rock","description":"d","creationContext":{"creator":{"groupId":"7"}}}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            String::from_utf8_lossy(&request).to_string()
        });
        let client = Client::new(Some(crate::ApiKey::new("k"))).with_apis_root(root);
        let meta = client.asset_metadata(42).unwrap();
        assert_eq!(
            meta,
            AssetMetadata {
                asset_id: 42,
                asset_type: "MeshPart".into(),
                display_name: "Rock".into(),
                description: "d".into(),
                creator: Some(crate::StoreCreator::Group(7)),
            }
        );
        let head = seen.join().unwrap();
        assert!(head.starts_with("GET /assets/v1/assets/42 "), "{head}");
        assert!(head.to_lowercase().contains("x-api-key: k"), "{head}");
    }
}
