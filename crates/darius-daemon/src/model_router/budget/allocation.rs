//! Atomic allocation operations for shared token budgets.
use super::{BudgetEnforcer, BudgetReservation, BudgetScope, RouterError, exceeded};

impl BudgetEnforcer {
    pub fn charge(&self, scope: BudgetScope, tokens: u64) -> Result<(), RouterError> {
        let mut budgets = self.budgets.lock();
        let (used, limit) = budgets
            .get_mut(&scope)
            .ok_or_else(|| exceeded(scope, 0, 0, tokens))?;
        if used.saturating_add(tokens) > *limit {
            return Err(exceeded(scope, *used, *limit, tokens));
        }
        *used = used.saturating_add(tokens);
        Ok(())
    }

    pub(crate) fn reserve(
        &self,
        scope: BudgetScope,
        input: u64,
        desired_output: u64,
    ) -> Result<BudgetReservation, RouterError> {
        let mut budgets = self.budgets.lock();
        let (used, limit) = budgets
            .get_mut(&scope)
            .ok_or_else(|| exceeded(scope, 0, 0, input))?;
        let available = limit.saturating_sub(*used);
        if input.saturating_add(1) > available {
            return Err(exceeded(scope, *used, *limit, input.saturating_add(1)));
        }
        let output = desired_output.min(available - input);
        let reserved = input + output;
        *used += reserved;
        Ok(BudgetReservation::new(
            self.clone(),
            scope,
            reserved,
            output,
        ))
    }
}
