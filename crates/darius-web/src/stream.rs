use crate::ServerState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use darius_cognitive::UiEvent;
#[derive(serde::Deserialize)]
pub(crate) struct Filter {
    task_id: String,
}
pub(crate) async fn events(
    State(state): State<ServerState>,
    Query(filter): Query<Filter>,
) -> Response {
    if !state.jobs.lock().unwrap().contains_key(&filter.task_id) {
        return (StatusCode::NOT_FOUND, "task not found").into_response();
    }
    // Replay the per-task journal before following it: POST-to-subscribe cannot lose fast runs.
    let stream = async_stream::stream! {
        let mut cursor = 0;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(20));
        loop {
            tick.tick().await;
            let batch = {
                let jobs = state.jobs.lock().unwrap();
                jobs[&filter.task_id].events[cursor..].to_vec()
            };
            for event in batch {
                cursor += 1;
                let terminal = matches!(event.event, UiEvent::Done | UiEvent::Error { .. });
                yield Ok::<_, std::convert::Infallible>(Event::default().event("ui")
                    .id(event.sequence.to_string()).data(serde_json::to_string(&event).unwrap()));
                if terminal { return; }
            }
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
