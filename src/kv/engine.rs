//! Synchronous storage engine: the key-value state and the semantics of every operation.
//!
//! Plain code with no tokio, channels or tasks, so the semantics are unit-tested directly.
//! The storage task owns the only instance.

use std::collections::HashMap;

/// In-memory key-value state.
#[derive(Debug, Default)]
pub struct Engine {
    entries: HashMap<String, String>,
}

impl Engine {
    /// Stores `value` under `key`, creating the key or overwriting its value.
    pub fn set(&mut self, key: String, value: String) {
        self.entries.insert(key, value);
    }

    /// Returns the value under `key`, `None` if the key is missing.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Replaces the value under an existing `key`. Returns `false` if the key is missing,
    /// in which case nothing is inserted.
    pub fn update(&mut self, key: &str, value: String) -> bool {
        match self.entries.get_mut(key) {
            Some(current) => {
                *current = value;
                true
            }
            None => false,
        }
    }

    /// Removes `key`. Returns `false` if the key is missing.
    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with(key: &str, value: &str) -> Engine {
        let mut engine = Engine::default();
        engine.set(key.to_owned(), value.to_owned());
        engine
    }

    #[test]
    fn get_missing_key_returns_none() {
        assert_eq!(Engine::default().get("a"), None);
    }

    #[test]
    fn set_creates_missing_key() {
        assert_eq!(engine_with("a", "1").get("a"), Some("1"));
    }

    #[test]
    fn set_overwrites_existing_value() {
        let mut engine = engine_with("a", "1");
        engine.set("a".to_owned(), "2".to_owned());
        assert_eq!(engine.get("a"), Some("2"));
    }

    #[test]
    fn update_replaces_existing_value() {
        let mut engine = engine_with("a", "1");
        assert!(engine.update("a", "2".to_owned()));
        assert_eq!(engine.get("a"), Some("2"));
    }

    #[test]
    fn update_missing_key_does_not_insert() {
        let mut engine = Engine::default();
        assert!(!engine.update("a", "1".to_owned()));
        assert_eq!(engine.get("a"), None);
    }

    #[test]
    fn delete_removes_existing_key() {
        let mut engine = engine_with("a", "1");
        assert!(engine.delete("a"));
        assert_eq!(engine.get("a"), None);
    }

    #[test]
    fn delete_missing_key_returns_false() {
        assert!(!Engine::default().delete("a"));
    }
}
