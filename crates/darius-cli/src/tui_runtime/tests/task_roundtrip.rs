use super::tool_evidence::*;
use super::*;
fn task_id(content: &str) -> &str {
    content
        .split_once('[')
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
}
#[test]
fn tui_task_board_tools_complete_actual_returned_id() {
    let server = Server::scripted(false, |r| match results(&r).len() {
        0 => reply("add", "task_add", json!({"title":"TASK_UNIQUE_Z9"})),
        1 => reply("list", "task_list", json!({})),
        2 => {
            let listed = result(&r, "list", "task_list");
            reply("complete", "task_complete", json!({"id":task_id(listed)}))
        }
        3 => reply("after", "task_list", json!({})),
        4 => json!({"role":"assistant","content":"done"}),
        _ => panic!("unexpected model continuation"),
    });
    let mut h = Harness::new(&server.url, None);
    h.submit("add, list, complete and verify");
    h.permit(PermissionChoice::AllowOnce);
    h.permit(PermissionChoice::AllowOnce);
    finish(&mut h);
    let requests: Vec<_> = (0..5)
        .map(|_| server.requests.recv_timeout(BOUND).unwrap())
        .collect();
    let added = result(&requests[1], "add", "task_add");
    assert!(added.contains("added task: TASK_UNIQUE_Z9"));
    let id = task_id(added);
    assert_eq!(
        result(&requests[2], "list", "task_list"),
        format!("○ [{id}] TASK_UNIQUE_Z9\n")
    );
    assert_eq!(
        result(&requests[3], "complete", "task_complete"),
        format!("completed: {id}")
    );
    assert_eq!(
        result(&requests[4], "after", "task_list"),
        format!("● [{id}] TASK_UNIQUE_Z9\n")
    );
    h.shutdown();
}
