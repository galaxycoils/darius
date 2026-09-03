//! Shared token budgets for router and live model calls.
use super::RouterError;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BudgetScope {
    Session,
    Subagent,
    Global,
    Eval,
}
#[derive(Clone)]
pub struct BudgetEnforcer {
    budgets: Arc<Mutex<HashMap<BudgetScope, (u64, u64)>>>,
}
impl BudgetEnforcer {
    pub fn new() -> Self {
        Self::from_limits([
            (BudgetScope::Session, 100_000),
            (BudgetScope::Subagent, 50_000),
            (BudgetScope::Global, 1_000_000),
            (BudgetScope::Eval, 10_000),
        ])
    }
    pub fn with_limit(scope: BudgetScope, limit: u64) -> Self {
        Self::from_limits([(scope, limit)])
    }
    fn from_limits(limits: impl IntoIterator<Item = (BudgetScope, u64)>) -> Self {
        let map = limits.into_iter().map(|(s, n)| (s, (0, n))).collect();
        Self {
            budgets: Arc::new(Mutex::new(map)),
        }
    }
    pub fn check_budget(&self, scope: BudgetScope, requested: u64) -> Result<(), RouterError> {
        let (used, limit) = self.budgets.lock().get(&scope).copied().unwrap_or((0, 0));
        if used.saturating_add(requested) > limit {
            return Err(RouterError::BudgetExceeded(format!(
                "scope {scope:?}: {used}/{limit} tokens used, estimated {requested}"
            )));
        }
        Ok(())
    }
    pub fn record_usage(&self, scope: BudgetScope, tokens: u64) {
        if let Some((used, _)) = self.budgets.lock().get_mut(&scope) {
            *used = used.saturating_add(tokens);
        }
    }
    pub fn remaining(&self, scope: BudgetScope) -> u64 {
        let budgets = self.budgets.lock();
        budgets
            .get(&scope)
            .map_or(0, |(used, limit)| limit.saturating_sub(*used))
    }
}
impl Default for BudgetEnforcer {
    fn default() -> Self {
        Self::new()
    }
}
