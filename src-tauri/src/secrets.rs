//! Secrets: the API keys, tokens and passwords Heyo is given, by whose
//! they are and their name. A model provider's key is `provider:<id>/key`;
//! a connection's password is `connection:<id>/password`.
//!
//! This is the one place they are kept and found. For now that is the
//! settings file (`settings.json`, which only the user may read: mode
//! 0600), under `secrets`, beside the settings and not inside the things
//! they belong to: a provider's configuration and a connection's hold none,
//! so whatever shows those cannot show a secret by mistake. A keyring
//! would take the place of [`Secrets`] here and nowhere else.
//!
//! What a secret holds goes to whoever needs it to do its work (a model's
//! adapter, a connector), and never to a window or the log: a window is
//! told only which are set ([`Secrets::names`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Whose a secret is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner<'a> {
    /// A model provider, by its id (`models::registry`).
    Provider(&'a str),
    /// A connection's account, by its id (`connections`).
    #[cfg_attr(not(test), allow(dead_code))]
    Connection(&'a str),
}

impl Owner<'_> {
    /// What a secret's key begins with.
    fn prefix(self) -> String {
        match self {
            Owner::Provider(id) => format!("provider:{id}/"),
            Owner::Connection(id) => format!("connection:{id}/"),
        }
    }

    fn key(self, name: &str) -> String {
        self.prefix() + name
    }
}

/// The secrets, by `<owner>/<name>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secrets(BTreeMap<String, String>);

impl Secrets {
    /// The secret of that name, if it is set.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read(&self, owner: Owner, name: &str) -> Option<&str> {
        self.0.get(&owner.key(name)).map(String::as_str)
    }

    /// Keeps `value` as the secret of that name, trimmed. An empty one is a
    /// secret removed.
    pub fn store(&mut self, owner: Owner, name: &str, value: &str) {
        match value.trim() {
            "" => self.delete(owner, name),
            value => drop(self.0.insert(owner.key(name), value.to_owned())),
        }
    }

    pub fn delete(&mut self, owner: Owner, name: &str) {
        self.0.remove(&owner.key(name));
    }

    /// Removes every secret of `owner`: it is gone.
    pub fn forget(&mut self, owner: Owner) {
        let prefix = owner.prefix();
        self.0.retain(|key, _| !key.starts_with(&prefix));
    }

    /// The names of the secrets `owner` has set, in order: what a window
    /// may be told of them.
    pub fn names(&self, owner: Owner) -> Vec<String> {
        let prefix = owner.prefix();
        self.0.keys().filter_map(|key| key.strip_prefix(&prefix)).map(str::to_owned).collect()
    }

    /// Every secret of `owner`, by name: for handing to what uses them.
    pub fn of(&self, owner: Owner) -> BTreeMap<String, String> {
        let prefix = owner.prefix();
        self.0.iter().filter_map(|(key, value)| Some((key.strip_prefix(&prefix)?.to_owned(), value.clone()))).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_kept_by_whose_it_is_and_its_name() {
        let mut secrets = Secrets::default();
        secrets.store(Owner::Provider("claude"), "key", " sk-ant-1 ");
        secrets.store(Owner::Connection("work"), "password", "hunter2");
        secrets.store(Owner::Connection("work"), "token", "t");
        assert_eq!(secrets.read(Owner::Provider("claude"), "key"), Some("sk-ant-1"));
        // The same name of another owner is another secret.
        assert_eq!(secrets.read(Owner::Connection("claude"), "key"), None);
        assert_eq!(secrets.read(Owner::Provider("work"), "password"), None);
        assert_eq!(secrets.names(Owner::Connection("work")), ["password", "token"]);
        assert_eq!(secrets.of(Owner::Connection("work"))["token"], "t");
        // An owner whose id begins another's is not that one.
        secrets.store(Owner::Connection("work-2"), "password", "other");
        assert_eq!(secrets.names(Owner::Connection("work")), ["password", "token"]);
        // An empty one is removed.
        secrets.store(Owner::Connection("work"), "token", "  ");
        assert_eq!(secrets.names(Owner::Connection("work")), ["password"]);
        secrets.forget(Owner::Connection("work"));
        assert!(secrets.names(Owner::Connection("work")).is_empty());
        assert_eq!(secrets.read(Owner::Connection("work-2"), "password"), Some("other"));
        secrets.delete(Owner::Connection("work-2"), "password");
        secrets.delete(Owner::Provider("claude"), "key");
        assert!(secrets.is_empty());
    }

    #[test]
    fn they_are_written_as_one_table() {
        let mut secrets = Secrets::default();
        secrets.store(Owner::Provider("claude"), "key", "sk");
        secrets.store(Owner::Connection("work"), "password", "pw");
        let written = serde_json::to_value(&secrets).unwrap();
        assert_eq!(written, serde_json::json!({ "connection:work/password": "pw", "provider:claude/key": "sk" }));
        assert_eq!(serde_json::from_value::<Secrets>(written).unwrap(), secrets);
    }
}
