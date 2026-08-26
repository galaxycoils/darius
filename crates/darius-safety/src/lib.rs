//! Safety gates, capability tiers, and audit log.
//!
//! Capability-based access control with approval tiers and tamper-evident audit.
//! Untrusted subagents are confined to isolation tier T2+; eval/learn paths
//! cannot bypass safety.

pub mod capabilities;
pub mod gates;
pub mod audit;
