//! `search_asset`, with Studio's arguments and answer. What Open Cloud
//! reaches with the editor's key:
//!
//! - the Creator Store, through the Creator Store API
//!   (`toolbox-service/v2/assets:search`, scope `creator-store-product:read`);
//! - the key owner's inventory, through `cloud/v2/users/{id}/inventory-items`
//!   (scope `user.inventory-item:read`).
//!
//! Open Cloud lists neither a group's nor a universe's inventory. A group
//! search therefore finds what the group published on the Creator Store, and
//! says so in its `nextSteps`; a universe search is refused with the reason.
//! In `auto`, the universe is skipped with that reason in `skippedSources`.

mod answer;

use rbx_cloud::{Client, StoreCategory, StoreCreator, StoreSearch};
use serde_json::Value;

use super::super::{text_argument, Outcome};
use super::{client, key_owner};
use crate::mcp::Session;
use answer::{Answer, Context};

pub(super) const NAME: &str = "search_asset";

const DEFAULT_RESULTS: u64 = 5;
const MAX_RESULTS: u64 = 20;
/// Each inventory page is named one asset at a time; past this many pages
/// the answer would outlast the client's wait.
const MAX_INVENTORY_PAGES: usize = 3;

/// `assetType`'s values, with the Creator Store category and the Open Cloud
/// inventory types each one searches.
const TYPES: [(&str, StoreCategory, &[&str]); 8] = [
    ("Model", StoreCategory::Model, &["MODEL", "PACKAGE"]),
    ("Audio", StoreCategory::Audio, &["AUDIO"]),
    ("Mesh", StoreCategory::MeshPart, &["MESH_PART"]),
    ("MeshPart", StoreCategory::MeshPart, &["MESH_PART"]),
    ("Image", StoreCategory::Decal, &["DECAL"]),
    ("Decal", StoreCategory::Decal, &["DECAL"]),
    ("Video", StoreCategory::Video, &["VIDEO"]),
    ("Package", StoreCategory::Model, &["PACKAGE"]),
];

pub(super) fn run(arguments: &Value, session: &mut Session) -> Outcome {
    let request = match Request::parse(arguments, session.studio.place_id) {
        Ok(request) => request,
        Err(refusal) => return Ok(refusal.into_value()),
    };
    session.deferred = Some(Box::new(move || Ok(request.run().into_value())));
    Ok(Value::Null)
}

/// A refusal made before any request, boxed as `Answer` is large.
fn refusal(why: String, steps: &[&str]) -> Box<Answer> {
    Box::new(Answer::error(why, steps))
}

#[derive(Debug)]
enum GroupPick {
    Id(u64),
    Name(String),
}

#[derive(Debug)]
pub(super) struct Request {
    query: String,
    kind: usize,
    scope: String,
    include: Option<Vec<String>>,
    exclude: Vec<String>,
    group: Option<GroupPick>,
    facets: Vec<String>,
    min_price: Option<u64>,
    max_price: Option<u64>,
    verified_only: bool,
    audio: (Option<u32>, Option<u32>),
    max_results: usize,
    place_id: Option<u64>,
}

fn strings(arguments: &Value, name: &str) -> Option<Vec<String>> {
    arguments.get(name).and_then(Value::as_array).map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

fn number(arguments: &Value, name: &str) -> Option<u64> {
    arguments
        .get(name)
        .and_then(Value::as_f64)
        .filter(|n| *n >= 0.0)
        .map(|n| n.round() as u64)
}

impl Request {
    fn parse(arguments: &Value, place_id: Option<u64>) -> Result<Self, Box<Answer>> {
        // Studio searches its default type for one it does not know.
        let kind = text_argument(arguments, "assetType")
            .and_then(|name| {
                TYPES
                    .iter()
                    .position(|(known, ..)| known.eq_ignore_ascii_case(name.trim()))
            })
            .unwrap_or(0);
        let scope = text_argument(arguments, "scope")
            .unwrap_or("auto")
            .to_owned();
        if !["auto", "creator_store", "user", "group", "universe"].contains(&scope.as_str()) {
            return Err(refusal(
                format!("Unknown scope \"{scope}\"."),
                &["Use auto, creator_store, user, group or universe."],
            ));
        }
        let group =
            match (
                text_argument(arguments, "groupId"),
                text_argument(arguments, "groupName"),
            ) {
                (Some(_), Some(_)) => {
                    return Err(refusal(
                        "groupId and groupName are mutually exclusive.".to_owned(),
                        &["Pass only one of them."],
                    ))
                }
                (Some(id), None) => Some(GroupPick::Id(id.trim().parse().map_err(|_| {
                    refusal(format!("groupId must be numeric, not \"{id}\"."), &[])
                })?)),
                (None, Some(name)) => Some(GroupPick::Name(name.trim().to_owned())),
                (None, None) => None,
            };
        if group.is_some() && scope != "group" {
            return Err(refusal(
                format!(
                    "groupId/groupName only apply when scope is \"group\"; scope is \"{scope}\"."
                ),
                &["Set scope='group' to search the specified group, or remove the groupId/groupName argument."],
            ));
        }
        let (mut min_price, mut max_price) = (
            number(arguments, "minPriceCents"),
            number(arguments, "maxPriceCents"),
        );
        match text_argument(arguments, "priceFilter") {
            Some("free") => max_price = Some(0),
            Some("paid") => min_price = Some(min_price.unwrap_or(0).max(1)),
            _ => {}
        }
        let mut facets = strings(arguments, "facets").unwrap_or_default();
        facets.extend(strings(arguments, "tags").unwrap_or_default());
        Ok(Request {
            query: text_argument(arguments, "query")
                .unwrap_or("")
                .trim()
                .to_owned(),
            kind,
            scope,
            include: strings(arguments, "includeSources"),
            exclude: strings(arguments, "excludeSources").unwrap_or_default(),
            group,
            facets,
            min_price,
            max_price,
            verified_only: arguments
                .get("verifiedCreatorsOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            audio: (
                number(arguments, "audioMinDuration").map(|s| s as u32),
                number(arguments, "audioMaxDuration").map(|s| s as u32),
            ),
            max_results: number(arguments, "maxResults")
                .unwrap_or(DEFAULT_RESULTS)
                .clamp(1, MAX_RESULTS) as usize,
            place_id,
        })
    }

    fn run(self) -> Answer {
        if self.scope == "universe" {
            return Answer::universe_refusal(self.place_id.is_some());
        }
        let client = match client() {
            Ok(client) => client,
            Err(why) => return Answer::error(why, &[]),
        };
        let owner = match key_owner(&client) {
            Ok(owner) => owner,
            Err(why) => return Answer::error(why, &[]),
        };
        let context = Context::of(&client, owner, self.place_id);
        let mut answer = Answer::new(&self.scope, &self.query, &context);
        match self.scope.as_str() {
            "creator_store" => self.store(&client, None, &mut answer),
            "user" => self.inventory(&client, owner, &mut answer),
            "group" => self.group(&client, &context, &mut answer),
            _ => self.auto(&client, owner, &context, &mut answer),
        }
        answer
    }

    fn wants(&self, source: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|only| only.iter().any(|s| s == source))
            && !self.exclude.iter().any(|s| s == source)
    }

    fn auto(&self, client: &Client, owner: u64, context: &Context, answer: &mut Answer) {
        if self.wants("universe") {
            answer.skip("universe", context.universe_skip_reason());
        }
        if self.wants("group") {
            match context.owning_group() {
                Some(group) => self.store(client, Some(StoreCreator::Group(group)), answer),
                None => answer.skip("group", "this universe is not owned by a group"),
            }
            answer.list_groups(client);
        }
        if self.wants("user") && answer.wants_more(self.max_results) {
            self.inventory(client, owner, answer);
        }
        if self.wants("creator_store") && answer.wants_more(self.max_results) {
            self.store(client, None, answer);
        }
    }

    fn group(&self, client: &Client, context: &Context, answer: &mut Answer) {
        let groups = answer.list_groups(client);
        let target = match &self.group {
            Some(GroupPick::Id(id)) => Some(*id),
            Some(GroupPick::Name(name)) => {
                match groups.iter().find(|g| g.name.eq_ignore_ascii_case(name)) {
                    Some(group) => Some(group.id),
                    None => {
                        answer.next_steps(&[
                            &format!("No group you manage is named \"{name}\"."),
                            "Pass scope='group' with groupId or groupName from the listed groups to retry.",
                        ]);
                        return;
                    }
                }
            }
            None => context.owning_group(),
        };
        match target {
            Some(group) => self.store(client, Some(StoreCreator::Group(group)), answer),
            None => answer.next_steps(&[
                "This universe is not owned by a group, and no groupId/groupName was supplied.",
                "Pass scope='group' with groupId or groupName from the listed groups to retry.",
            ]),
        }
    }

    /// The Creator Store, or what one group published there.
    fn store(&self, client: &Client, group: Option<StoreCreator>, answer: &mut Answer) {
        let source = if group.is_some() {
            "group"
        } else {
            "creator_store"
        };
        let (category, sub_types) = match TYPES[self.kind].0 {
            "Package" => (StoreCategory::Model, vec!["Package".to_owned()]),
            _ => (TYPES[self.kind].1, Vec::new()),
        };
        let search = StoreSearch {
            query: self.query.clone(),
            category: Some(category),
            creator: group,
            min_price_cents: self.min_price,
            max_price_cents: self.max_price,
            verified_only: self.verified_only,
            model_sub_types: sub_types,
            facets: self.facets.clone(),
            audio_min_seconds: self.audio.0,
            audio_max_seconds: self.audio.1,
            page_size: answer.room(self.max_results) as u32,
            page_token: None,
        };
        match client.search_creator_store(&search) {
            Ok(page) => answer.add_store(source, page, TYPES[self.kind].0),
            Err(err) => answer.source_failed(source, &err, "creator-store-product:read"),
        }
    }

    /// The inventory answers ids and types, and no query: names come from
    /// the Creator Store's details, or the Assets API for what never went on
    /// sale, and the query is matched against them here.
    fn inventory(&self, client: &Client, owner: u64, answer: &mut Answer) {
        let words: Vec<String> = self
            .query
            .split(['+', ' '])
            .map(|word| word.trim_matches('"').to_lowercase())
            .filter(|word| !word.is_empty())
            .collect();
        let mut token: Option<String> = None;
        let mut found = Vec::new();
        for _ in 0..MAX_INVENTORY_PAGES {
            let page =
                match client.inventory_assets(owner, TYPES[self.kind].2, 100, token.as_deref()) {
                    Ok(page) => page,
                    Err(err) => {
                        return answer.source_failed("user", &err, "user.inventory-item:read")
                    }
                };
            // With no query to match, only the rows that will be shown are
            // worth naming: each name is a request of its own.
            let wanted = if words.is_empty() {
                let room = answer.room(self.max_results).saturating_sub(found.len());
                &page.assets[..page.assets.len().min(room)]
            } else {
                &page.assets[..]
            };
            for row in answer::inventory_rows(client, owner, wanted) {
                let name = row["name"].as_str().unwrap_or("").to_lowercase();
                if words.iter().all(|word| name.contains(word.as_str())) {
                    found.push(row);
                }
            }
            match page.next_page_token {
                Some(next) if found.len() < answer.room(self.max_results) => token = Some(next),
                _ => break,
            }
        }
        answer.add_rows("user", found, self.max_results);
    }
}
