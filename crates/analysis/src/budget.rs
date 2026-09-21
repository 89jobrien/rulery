//! Global analysis state budget with zero-cost cache hits.

use std::collections::BTreeMap;

/// Result of one budgeted analysis state query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BudgetResult<T> {
    /// Query completed or was served from cache.
    Value(T),
    /// The next uncached state would exceed the limit.
    Inconclusive {
        /// Charged states.
        examined: u64,
        /// Configured state limit.
        limit: u64,
    },
}

/// Global per-command state budget.
#[derive(Clone, Debug)]
pub struct AnalysisBudget<K, V> {
    limit: u64,
    examined: u64,
    cache: BTreeMap<K, V>,
    exhausted: bool,
}

impl<K: Ord + Clone, V: Clone> AnalysisBudget<K, V> {
    /// Creates a budget with no charged states.
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            examined: 0,
            cache: BTreeMap::new(),
            exhausted: false,
        }
    }

    /// Resolves one state, charging only uncached queries.
    pub fn resolve(&mut self, key: K, query: impl FnOnce() -> V) -> BudgetResult<V> {
        if let Some(value) = self.cache.get(&key) {
            return BudgetResult::Value(value.clone());
        }
        if self.exhausted || self.examined >= self.limit {
            self.exhausted = true;
            return BudgetResult::Inconclusive {
                examined: self.examined,
                limit: self.limit,
            };
        }
        let value = query();
        self.examined += 1;
        self.cache.insert(key, value.clone());
        BudgetResult::Value(value)
    }

    /// Returns the number of charged states.
    #[must_use]
    pub const fn examined(&self) -> u64 {
        self.examined
    }
}
