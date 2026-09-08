//! Execution policy is independent of approval state and model instructions.
pub use darius_core::runtime_protocol::Mode as ExecutionPolicy;
pub fn denial(
    policy: ExecutionPolicy,
    risk: Option<darius_tools::ToolRisk>,
) -> Option<&'static str> {
    match (policy, risk) {
        (
            ExecutionPolicy::Plan,
            Some(darius_tools::ToolRisk::Mutating | darius_tools::ToolRisk::Shell),
        ) => Some("Plan mode denied: mutation and shell execution; use Auto with approval"),
        _ => None,
    }
}
