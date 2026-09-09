use crate::{ServerState, TaskEvent, TaskState};
use darius_cognitive::{EventSink, UiEvent};
use std::sync::Arc;
struct Sink {
    state: ServerState,
    id: String,
}
impl EventSink for Sink {
    fn emit(&self, event: UiEvent) {
        // The joined execution result, including headless denial, owns termination.
        if !matches!(event, UiEvent::Done | UiEvent::Error { .. }) {
            publish(&self.state, &self.id, event);
        }
    }
}
fn publish(state: &ServerState, id: &str, event: UiEvent) {
    let mut jobs = state.jobs.lock().unwrap();
    let job = jobs.get_mut(id).unwrap();
    let event = TaskEvent {
        task_id: id.into(),
        sequence: job.events.len(),
        event,
    };
    job.events.push(event.clone());
    let _ = state.event_sender.send(event);
}
pub(crate) async fn run(state: ServerState, id: String, goal: String) {
    let executor = state.executor.clone().expect("checked at submission");
    state.jobs.lock().unwrap().get_mut(&id).unwrap().task.state = TaskState::Running;
    publish(
        &state,
        &id,
        UiEvent::A2aTask {
            task_id: id.clone(),
            state: "Running".into(),
        },
    );
    let sink = Arc::new(Sink {
        state: state.clone(),
        id: id.clone(),
    });
    let result = tokio::task::spawn_blocking(move || executor(goal, sink))
        .await
        .unwrap_or_else(|error| Err(format!("execution worker failed: {error}")));
    let (status, output, terminal) = match result {
        Ok(output) => (TaskState::Completed, output, UiEvent::Done),
        Err(message) => (
            TaskState::Failed,
            message.clone(),
            UiEvent::Error { message },
        ),
    };
    {
        let mut jobs = state.jobs.lock().unwrap();
        let job = jobs.get_mut(&id).unwrap();
        job.task.state = status.clone();
        job.task.output = Some(output);
    }
    publish(
        &state,
        &id,
        UiEvent::A2aTask {
            task_id: id.clone(),
            state: format!("{status:?}"),
        },
    );
    publish(&state, &id, terminal);
}
