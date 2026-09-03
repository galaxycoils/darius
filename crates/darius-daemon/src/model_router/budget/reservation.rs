//! RAII request reservation: release before dispatch, charge after dispatch.
use super::{BudgetEnforcer, BudgetScope};

pub(crate) struct BudgetReservation {
    enforcer: BudgetEnforcer,
    scope: BudgetScope,
    reserved: u64,
    output: u64,
    dispatched: bool,
    settled: bool,
}

impl BudgetReservation {
    pub(super) fn new(
        enforcer: BudgetEnforcer,
        scope: BudgetScope,
        reserved: u64,
        output: u64,
    ) -> Self {
        Self {
            enforcer,
            scope,
            reserved,
            output,
            dispatched: false,
            settled: false,
        }
    }
    pub(crate) fn output_limit(&self) -> u64 {
        self.output
    }
    pub(crate) fn mark_dispatched(&mut self) {
        self.dispatched = true;
    }
    pub(crate) fn was_dispatched(&self) -> bool {
        self.dispatched
    }
    pub(crate) fn reconcile(mut self, actual: u64) {
        self.replace_reserved(actual.max(u64::from(self.dispatched)));
        self.settled = true;
    }
    pub(crate) fn conservative_charge(mut self) {
        self.settled = true;
    }
    fn replace_reserved(&self, actual: u64) {
        if let Some((used, _)) = self.enforcer.budgets.lock().get_mut(&self.scope) {
            *used = used.saturating_sub(self.reserved).saturating_add(actual);
        }
    }
}

impl Drop for BudgetReservation {
    fn drop(&mut self) {
        if !self.settled && !self.dispatched {
            self.replace_reserved(0);
        }
    }
}
