//! The items connectors sync, and where they are kept and searched.
//!
//! The store itself is the index shared with the clipboard history, which
//! is being drawn out of it as a module of its own. Until it is there,
//! [`Items`] here is a stand-in in memory with the same names and the same
//! promises (what `put` returned from is written; `search` answers from
//! what is there), so that everything around it is written and tested
//! against the interface it will have. The types of an item are the wire's
//! (`host.items.put`), declared here once.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorItemKind")]
pub enum ItemKind {
    Email,
    Event,
    Message,
    Document,
    Task,
}

/// How someone has to do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorRole")]
pub enum Role {
    From,
    To,
    Cc,
    Organizer,
    Attendee,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorPerson")]
pub struct Person {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// An e-mail address, or whatever the service knows them by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub address: Option<String>,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorAttachment")]
pub struct Attachment {
    pub name: String,
    /// In bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub size: Option<f64>,
    /// Its media type.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "type")]
    pub media_type: Option<String>,
}

/// One thing a connector found: a mail, an event, a message, a document,
/// a task. Its `id` is the connector's, unique within the account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorItem")]
pub struct Item {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub people: Option<Vec<Person>>,
    /// When it was sent, begins, was changed: ISO 8601.
    pub time: String,
    /// When it ends (an event): ISO 8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<String>,
    /// Its text, or as much of it as is worth finding it by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    /// Where it is opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attachments: Option<Vec<Attachment>>,
    /// The mailbox, calendar, channel or folder it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    /// `unread`, `flagged`, `done`...: the connector's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flags: Option<Vec<String>>,
}

/// An item a search found, and whose it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorFound")]
pub struct Found {
    pub account: String,
    pub item: Item,
    pub score: f64,
}

/// The stand-in for the shared index: items by account and id, with when
/// each was last written.
#[derive(Debug, Default)]
pub struct Items {
    kept: Mutex<BTreeMap<String, BTreeMap<String, (Item, DateTime<Utc>)>>>,
}

pub type Result<T> = std::result::Result<T, String>;

impl Items {
    /// The store at `path`. (The stand-in keeps nothing there.)
    pub fn open(_path: &Path) -> Result<Items> {
        Ok(Items::default())
    }

    /// Writes `items` into the account's part, each in place of the one
    /// of its id. Written when it returns.
    pub fn put(&self, account: &str, items: &[Item]) -> Result<()> {
        let now = Utc::now();
        let mut kept = self.kept.lock().unwrap();
        let mine = kept.entry(account.to_owned()).or_default();
        for item in items {
            mine.insert(item.id.clone(), (item.clone(), now));
        }
        Ok(())
    }

    pub fn remove(&self, account: &str, ids: &[String]) -> Result<()> {
        if let Some(mine) = self.kept.lock().unwrap().get_mut(account) {
            for id in ids {
                mine.remove(id);
            }
        }
        Ok(())
    }

    /// Everything of the account.
    pub fn drop_account(&self, account: &str) -> Result<()> {
        self.kept.lock().unwrap().remove(account);
        Ok(())
    }

    /// Removes the account's items last written before `older_than`; says
    /// how many.
    pub fn prune(&self, account: &str, older_than: DateTime<Utc>) -> Result<usize> {
        let mut kept = self.kept.lock().unwrap();
        let Some(mine) = kept.get_mut(account) else { return Ok(0) };
        let before = mine.len();
        mine.retain(|_, (_, written)| *written >= older_than);
        Ok(before - mine.len())
    }

    pub fn count(&self, account: &str) -> Result<usize> {
        Ok(self.kept.lock().unwrap().get(account).map_or(0, BTreeMap::len))
    }

    /// The items that have every word of `query`, the best first, then the
    /// latest. (The stand-in matches words; the index ranks properly.)
    pub fn search(&self, query: &str, limit: usize, _now: DateTime<Local>) -> Result<Vec<Found>> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let kept = self.kept.lock().unwrap();
        let mut found: Vec<Found> = kept
            .iter()
            .flat_map(|(account, items)| items.values().map(move |(item, _)| (account, item)))
            .filter_map(|(account, item)| {
                let title = item.title.to_lowercase();
                let rest = [item.text.as_deref(), item.location.as_deref(), item.folder.as_deref()]
                    .into_iter()
                    .flatten()
                    .chain(item.people.iter().flatten().flat_map(|person| [person.name.as_deref(), person.address.as_deref()].into_iter().flatten()))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                let mut score = 0.0;
                for word in &words {
                    score += match () {
                        _ if title.contains(word.as_str()) => 2.0,
                        _ if rest.contains(word.as_str()) => 1.0,
                        _ => return None,
                    };
                }
                Some(Found { account: account.clone(), item: item.clone(), score })
            })
            .collect();
        found.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| b.item.time.cmp(&a.item.time)));
        found.truncate(limit);
        Ok(found)
    }
}

#[cfg(test)]
pub(crate) fn item(id: &str, title: &str) -> Item {
    Item {
        id: id.into(),
        kind: ItemKind::Email,
        title: title.into(),
        people: None,
        time: "2026-11-03T09:00:00Z".into(),
        end: None,
        text: None,
        url: None,
        location: None,
        attachments: None,
        folder: None,
        flags: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_item_is_read_as_connectors_write_it() {
        let written = json!({
            "id": "m1", "kind": "email", "title": "Invoice", "time": "2026-11-03T09:00:00Z",
            "people": [{ "name": "Ada", "address": "ada@example.com", "role": "from" }, { "role": "to", "address": "me@example.com" }],
            "attachments": [{ "name": "invoice.pdf", "size": 1024, "type": "application/pdf" }],
            "folder": "INBOX", "flags": ["unread"],
        });
        let item: Item = serde_json::from_value(written.clone()).unwrap();
        assert_eq!((item.kind, item.people.as_ref().unwrap()[1].role), (ItemKind::Email, Role::To));
        assert_eq!(item.attachments.as_ref().unwrap()[0].media_type.as_deref(), Some("application/pdf"));
        let mut again = serde_json::to_value(&item).unwrap();
        again["attachments"][0]["size"] = json!(1024);
        assert_eq!(again, written);
        // A kind that is none, or no time, is not an item.
        assert!(serde_json::from_value::<Item>(json!({ "id": "x", "kind": "tweet", "title": "t", "time": "" })).is_err());
        assert!(serde_json::from_value::<Item>(json!({ "id": "x", "kind": "task", "title": "t" })).is_err());
    }

    #[test]
    fn the_stand_in_keeps_items_by_account() {
        let items = Items::default();
        items.put("work", &[item("1", "Quarterly invoice"), item("2", "Lunch")]).unwrap();
        items.put("home", &[item("1", "Invoice for the bike")]).unwrap();
        items.put("work", &[item("2", "Lunch on Friday")]).unwrap();
        assert_eq!((items.count("work").unwrap(), items.count("home").unwrap()), (2, 1));
        let found = items.search("invoice", 10, Local::now()).unwrap();
        assert_eq!(found.len(), 2);
        items.remove("work", &["1".into()]).unwrap();
        assert_eq!(items.search("lunch friday", 10, Local::now()).unwrap()[0].item.title, "Lunch on Friday");
        assert_eq!(items.prune("work", Utc::now() + chrono::TimeDelta::seconds(1)).unwrap(), 1);
        items.drop_account("home").unwrap();
        assert_eq!(items.search("invoice", 10, Local::now()).unwrap(), []);
    }
}
