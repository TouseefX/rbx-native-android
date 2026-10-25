//! `search_asset`'s answer, in the fields and words Studio's own answers
//! use, sent as compact JSON text as Studio sends it.

use rbx_cloud::{
    Client, CloudError, Group, InventoryAsset, Owner, StoreAsset, StoreCreator, StorePage,
};
use serde_json::{json, Map, Value};

use super::super::describe;

const NEXT_STEPS: [&str; 4] = [
    "Review the results and choose the best asset for the user's request. Prefer inventory results (source='inventory') as these are already owned and always insertable.",
    "For free/owned assets: call insert_asset with assetId, assetType, and assetName from the chosen result to insert it.",
    "For paid assets (isFree=false): show the user the creatorStoreUrl link and priceLabel so they can purchase it. Do not attempt to insert paid assets \u{2014} they will fail.",
    "Some Creator Store assets may be restricted or moderated and fail to insert. If insert_asset fails, try the next result from the list rather than retrying the same asset.",
];

/// Said whenever a group search falls back to the Creator Store.
const GROUP_NOTE: &str = "Open Cloud has no group inventory listing, so the group's results are \
    what it published on the Creator Store.";

pub(super) struct Answer {
    fields: Map<String, Value>,
    results: Vec<Value>,
    sources: Vec<&'static str>,
    skipped: Map<String, Value>,
    groups: Option<Vec<Group>>,
    steps: Option<Vec<String>>,
    notes: Vec<String>,
}

impl Answer {
    pub(super) fn new(scope: &str, query: &str, context: &Context) -> Self {
        let mut fields = Map::new();
        fields.insert("status".into(), json!("success"));
        fields.insert("scope".into(), json!(scope));
        fields.insert("query".into(), json!(query));
        fields.insert("context".into(), context.to_json());
        Answer {
            fields,
            results: Vec::new(),
            sources: Vec::new(),
            skipped: Map::new(),
            groups: None,
            steps: None,
            notes: Vec::new(),
        }
    }

    /// Studio answers a search it cannot run as a result, not a tool
    /// error, so the model reads the reason and the way out.
    pub(super) fn error(message: String, steps: &[&str]) -> Self {
        let mut fields = Map::new();
        fields.insert("status".into(), json!("error"));
        fields.insert("error".into(), json!(message));
        Answer {
            fields,
            results: Vec::new(),
            sources: Vec::new(),
            skipped: Map::new(),
            groups: None,
            steps: Some(steps.iter().map(|s| (*s).to_owned()).collect()),
            notes: Vec::new(),
        }
    }

    /// No universe listing exists, so this needs no lookup: only whether the
    /// place is linked to a Roblox place changes the reason.
    pub(super) fn universe_refusal(published: bool) -> Self {
        match published {
            false => Answer::error(
                "scope='universe' is unavailable because this game is not yet published (no universe ID).".to_owned(),
                &[
                    "Use scope='auto' or scope='user' to search the user's personal inventory and Creator Store instead.",
                    "The game must be published to Roblox before its universe inventory becomes searchable.",
                ],
            ),
            true => Answer::error(
                "scope='universe' is unavailable in rbx-native: Open Cloud has no endpoint listing a universe's assets (its only inventory listing is a user's, cloud/v2/users/{id}/inventory-items).".to_owned(),
                &["Use scope='auto', 'user' or 'creator_store' instead."],
            ),
        }
    }

    pub(super) fn skip(&mut self, source: &str, reason: &str) {
        self.skipped.insert(source.to_owned(), json!(reason));
    }

    pub(super) fn next_steps(&mut self, steps: &[&str]) {
        self.steps = Some(steps.iter().map(|s| (*s).to_owned()).collect());
    }

    /// The groups the key's user manages, fetched once; none when the key
    /// may not list them.
    pub(super) fn list_groups(&mut self, client: &Client) -> Vec<Group> {
        self.groups
            .get_or_insert_with(|| client.manageable_groups().unwrap_or_default())
            .clone()
    }

    pub(super) fn wants_more(&self, max: usize) -> bool {
        self.results.len() < max
    }

    pub(super) fn room(&self, max: usize) -> usize {
        max.saturating_sub(self.results.len()).max(1)
    }

    pub(super) fn add_store(&mut self, source: &'static str, page: StorePage, asked: &str) {
        if source == "group" {
            self.notes.push(GROUP_NOTE.to_owned());
        }
        if !page.available_facets.is_empty() && !self.fields.contains_key("availableFacets") {
            self.fields
                .insert("availableFacets".into(), json!(page.available_facets));
        }
        let rows = page
            .assets
            .iter()
            .map(|asset| store_row(asset, asked))
            .collect();
        self.add_rows(source, rows, usize::MAX);
    }

    pub(super) fn add_rows(&mut self, source: &'static str, rows: Vec<Value>, max: usize) {
        self.sources.push(source);
        let room = max.saturating_sub(self.results.len());
        self.results.extend(rows.into_iter().take(room));
    }

    /// A source that failed is skipped in `auto`, and is the answer when it
    /// was the one asked for.
    pub(super) fn source_failed(&mut self, source: &str, err: &CloudError, scope: &str) {
        let why = describe(err, scope);
        if self.fields["scope"] == "auto" {
            self.skip(source, &why);
        } else {
            self.fields.insert("status".into(), json!("error"));
            self.fields.insert("error".into(), json!(why));
        }
    }

    pub(super) fn into_value(mut self) -> Value {
        let succeeded = self.fields["status"] == "success";
        if succeeded {
            self.fields
                .insert("resultCount".into(), json!(self.results.len()));
            self.fields.insert("results".into(), json!(self.results));
            if !self.sources.is_empty() {
                self.fields
                    .insert("searchSources".into(), json!(self.sources));
            }
        }
        if !self.skipped.is_empty() {
            self.fields
                .insert("skippedSources".into(), Value::Object(self.skipped));
        }
        if let Some(groups) = self.groups.filter(|groups| !groups.is_empty()) {
            let groups: Vec<Value> = groups
                .iter()
                .map(|g| json!({ "id": g.id, "name": g.name }))
                .collect();
            self.fields.insert("groups".into(), json!(groups));
        }
        let mut steps = match self.steps {
            Some(steps) => steps,
            None if succeeded => NEXT_STEPS.iter().map(|s| (*s).to_owned()).collect(),
            None => Vec::new(),
        };
        steps.extend(self.notes);
        if !steps.is_empty() {
            self.fields.insert("nextSteps".into(), json!(steps));
        }
        Value::String(Value::Object(self.fields).to_string())
    }
}

/// Who is searching and what place, as Studio's `context` says it.
pub(super) struct Context {
    user_id: u64,
    published: Option<Published>,
}

struct Published {
    universe_id: Option<u64>,
    owner: Option<Owner>,
    owner_name: Option<String>,
}

impl Context {
    /// A place counts as published once it is linked to a Roblox place;
    /// its universe and owner are looked up, and left out if that fails.
    pub(super) fn of(client: &Client, user_id: u64, place_id: Option<u64>) -> Self {
        let published = place_id.map(|place| {
            let universe_id = client.universe_of_place(place).ok().flatten();
            let owner = universe_id
                .and_then(|u| client.universe(u).ok())
                .map(|u| u.owner);
            let owner_name = match owner {
                Some(Owner::User(id)) => client.user_display_name(id).ok(),
                Some(Owner::Group(id)) => client
                    .manageable_groups()
                    .ok()
                    .and_then(|groups| groups.into_iter().find(|g| g.id == id))
                    .map(|g| g.name),
                None => None,
            };
            Published {
                universe_id,
                owner,
                owner_name,
            }
        });
        Context { user_id, published }
    }

    pub(super) fn owning_group(&self) -> Option<u64> {
        match self.published.as_ref()?.owner? {
            Owner::Group(id) => Some(id),
            Owner::User(_) => None,
        }
    }

    pub(super) fn universe_skip_reason(&self) -> &'static str {
        match self.published {
            None => "game is not published",
            Some(_) => "Open Cloud has no universe inventory listing",
        }
    }

    fn to_json(&self) -> Value {
        let mut context =
            json!({ "userId": self.user_id.to_string(), "isPublished": self.published.is_some() });
        if let Some(published) = &self.published {
            if let Some(universe) = published.universe_id {
                context["universeId"] = json!(universe.to_string());
            }
            if let Some(owner) = published.owner {
                let (kind, id) = match owner {
                    Owner::User(id) => ("User", id),
                    Owner::Group(id) => ("Group", id),
                };
                context["creatorType"] = json!(kind);
                context["creatorId"] = json!(id.to_string());
            }
            if let Some(name) = &published.owner_name {
                context["creatorName"] = json!(name);
            }
        }
        context
    }
}

fn thumbnail(id: u64) -> String {
    format!(
        "https://thumbnails.roblox.com/v1/assets?assetIds={id}&returnPolicy=PlaceHolder&size=150x150&format=Png"
    )
}

/// `Enum.AssetType`'s name for what the store sells; `asked` names what
/// the search was for when the id says nothing more precise.
fn type_name(id: u32, asked: &str) -> String {
    match (id, asked) {
        (10, "Package") => "Package",
        (1, _) => "Image",
        (3, _) => "Audio",
        (4, _) => "Mesh",
        (10, _) => "Model",
        (13, _) => "Decal",
        (24, _) => "Animation",
        (40, _) => "MeshPart",
        (62, _) => "Video",
        _ => asked,
    }
    .to_owned()
}

fn store_row(asset: &StoreAsset, asked: &str) -> Value {
    let cents = asset.price_cents.unwrap_or(0);
    let mut row = json!({
        "assetId": asset.id.to_string(),
        "name": asset.name,
        "assetType": type_name(asset.asset_type_id, asked),
        "creatorName": asset.creator_name,
        "creatorStoreUrl": format!("https://create.roblox.com/store/asset/{}", asset.id),
        "thumbnailUrl": thumbnail(asset.id),
        "isFree": cents == 0,
        "priceCents": cents,
        "source": "creator_store",
    });
    if !asset.description.is_empty() {
        row["description"] = json!(asset.description);
    }
    if cents > 0 {
        let amount = format!("{}.{:02}", cents / 100, cents % 100);
        row["priceLabel"] = json!(match asset.currency.as_str() {
            "" | "USD" => format!("${amount}"),
            currency => format!("{amount} {currency}"),
        });
    }
    row
}

/// Inventory rows named from the Creator Store, or the Assets API for what
/// was never listed there; both lookups run in parallel.
pub(super) fn inventory_rows(client: &Client, owner: u64, assets: &[InventoryAsset]) -> Vec<Value> {
    let ids: Vec<u64> = assets.iter().map(|asset| asset.asset_id).collect();
    let details = client.creator_store_assets(&ids);
    // What never went on sale is named by the Assets API instead.
    let unlisted: Vec<u64> = ids
        .iter()
        .zip(&details)
        .filter(|(_, detail)| detail.is_err())
        .map(|(&id, _)| id)
        .collect();
    let mut metadata = client.asset_metadatas(&unlisted).into_iter();
    assets
        .iter()
        .zip(details)
        .map(|(asset, detail)| {
            let (name, creator, creator_name) = match detail {
                Ok(store) => (store.name, store.creator, Some(store.creator_name)),
                Err(_) => match metadata.next() {
                    Some(Ok(meta)) => (meta.display_name, meta.creator, None),
                    _ => (String::new(), None, None),
                },
            };
            inventory_row(asset, owner, name, creator, creator_name)
        })
        .collect()
}

fn inventory_row(
    asset: &InventoryAsset,
    owner: u64,
    name: String,
    creator: Option<StoreCreator>,
    creator_name: Option<String>,
) -> Value {
    let creator_id = match creator {
        Some(StoreCreator::User(id) | StoreCreator::Group(id)) => id,
        None => owner,
    };
    let asset_type = match asset.asset_type.as_str() {
        "MODEL" => "Model",
        "PACKAGE" => "Package",
        "DECAL" => "Decal",
        "AUDIO" => "Audio",
        "MESH_PART" => "MeshPart",
        "VIDEO" => "Video",
        other => other,
    };
    let mut row = json!({
        "assetId": asset.asset_id.to_string(),
        "name": name,
        "assetType": asset_type,
        "creatorId": creator_id.to_string(),
        "thumbnailUrl": thumbnail(asset.asset_id),
        "isFree": true,
        "priceCents": 0,
        "location": "user",
        "source": "inventory",
    });
    if let Some(creator_name) = creator_name.filter(|name| !name.is_empty()) {
        row["creatorName"] = json!(creator_name);
    }
    row
}
