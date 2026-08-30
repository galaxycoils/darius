//! Evaluation fixtures — test cases for the eval flywheel.

use serde::{Deserialize, Serialize};

/// A single evaluation fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalFixture {
    pub id: String,
    pub name: String,
    pub input: String,
    pub expected: String,
    pub category: String,
}

/// Store for evaluation fixtures.
#[derive(Debug, Default)]
pub struct FixtureStore {
    fixtures: Vec<EvalFixture>,
}

impl FixtureStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fixture: EvalFixture) {
        self.fixtures.push(fixture);
    }

    pub fn get(&self, id: &str) -> Option<&EvalFixture> {
        self.fixtures.iter().find(|f| f.id == id)
    }

    pub fn list(&self) -> &[EvalFixture] {
        &self.fixtures
    }

    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fixtures.is_empty()
    }

    pub fn by_category(&self, category: &str) -> Vec<&EvalFixture> {
        self.fixtures
            .iter()
            .filter(|f| f.category == category)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_add_and_get() {
        let mut store = FixtureStore::new();
        store.add(EvalFixture {
            id: "f1".into(),
            name: "test".into(),
            input: "input".into(),
            expected: "expected".into(),
            category: "coding".into(),
        });

        assert_eq!(store.len(), 1);
        let fixture = store.get("f1").unwrap();
        assert_eq!(fixture.name, "test");
    }

    #[test]
    fn store_by_category() {
        let mut store = FixtureStore::new();
        store.add(EvalFixture {
            id: "f1".into(),
            name: "a".into(),
            input: "i".into(),
            expected: "e".into(),
            category: "coding".into(),
        });
        store.add(EvalFixture {
            id: "f2".into(),
            name: "b".into(),
            input: "i".into(),
            expected: "e".into(),
            category: "reasoning".into(),
        });

        assert_eq!(store.by_category("coding").len(), 1);
        assert_eq!(store.by_category("reasoning").len(), 1);
    }
}
