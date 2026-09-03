mod allocation;
mod reservation;
use super::RouterError;
use parking_lot::Mutex;
pub(crate) use reservation::BudgetReservation;
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
    pub(crate) budgets: Arc<Mutex<HashMap<BudgetScope, (u64, u64)>>>,
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
        let budgets = limits.into_iter().map(|(s, n)| (s, (0, n))).collect();
        Self {
            budgets: Arc::new(Mutex::new(budgets)),
        }
    }

    pub fn remaining(&self, scope: BudgetScope) -> u64 {
        self.budgets
            .lock()
            .get(&scope)
            .map_or(0, |(used, limit)| limit.saturating_sub(*used))
    }
}

impl Default for BudgetEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

fn exceeded(scope: BudgetScope, used: u64, limit: u64, requested: u64) -> RouterError {
    RouterError::BudgetExceeded(format!(
        "scope {scope:?}: {used}/{limit} tokens used, estimated {requested}"
    ))
}
