//! Fresh isolated conversation per HTTP goal, with the same runtime and safety as `run`.
use crate::{
    paths::DariusPaths,
    runtime::{RuntimeOptions, SessionRuntime, block_on_turn},
};
use darius_cognitive::{AgentLoop, EventSink};
use darius_web::{GoalExecutor, ServerState};
use std::sync::{Arc, atomic::Ordering};
pub fn server_state(
    paths: DariusPaths,
    profile: String,
    offline: bool,
) -> Result<ServerState, String> {
    // Resolve now, before binding; setup/offline must never become fake execution.
    let runtime = SessionRuntime::from_options(&paths, &profile, RuntimeOptions { offline })
        .map_err(|error| error.to_string())?;
    if runtime.is_setup() || runtime.is_offline_demo() {
        return Err("Web execution unavailable: configure a live provider; offline mode does not execute goals".into());
    }
    drop(runtime);
    let executor: GoalExecutor = Arc::new(move |goal, sink| {
        let runtime = SessionRuntime::from_options(&paths, &profile, RuntimeOptions { offline })
            .map_err(|error| error.to_string())?;
        execute(runtime, &goal, sink)
    });
    Ok(ServerState::with_executor(executor))
}
fn execute(
    mut runtime: SessionRuntime,
    goal: &str,
    sink: Arc<dyn EventSink>,
) -> Result<String, String> {
    if runtime.is_setup() || runtime.is_offline_demo() {
        return Err("execution unavailable: runtime no longer configured".into());
    }
    let control = Arc::new(crate::permissions::HeadlessRunControl::default());
    let agent = AgentLoop::new(sink, control.clone());
    let workspace = runtime.workspace.to_string_lossy().into_owned();
    let result = block_on_turn(agent.run_turn_with_extra_tools(
        &runtime.metadata,
        &runtime.policy,
        goal,
        &mut runtime.conversation,
        runtime.model.as_mut(),
        &runtime.tools,
        &runtime.memory,
        &workspace,
        &runtime.dynamic_tool_specs,
    ));
    if control.0.load(Ordering::Relaxed) {
        return Err(
            "Mutation denied: web execution requires interactive approval; use darius tui".into(),
        );
    }
    result.map_err(|error| error.to_string())
}
