use crate::ServerState;
use axum::{extract::State, response::Html};
pub(crate) async fn dashboard(State(state): State<ServerState>) -> Html<&'static str> {
    if state.executor.is_none() {
        return Html("<h1>Darius</h1><p>Execution unavailable: no runtime configured.</p>");
    }
    Html(
        r#"<!doctype html><html><head><title>Darius</title></head><body>
<h1>Darius — headless execution</h1><p>Mutations require the interactive TUI.</p>
<form id="goalForm"><input id="goal" required><button>Run goal</button></form>
<pre id="events"></pre><script>
const output = document.getElementById('events');
document.getElementById('goalForm').onsubmit = async e => {
 e.preventDefault();
 try {
  const response = await fetch('/api/goal', {method:'POST', headers:{'Content-Type':'application/json'},
   body:JSON.stringify({goal:document.getElementById('goal').value})});
  const task = await response.json();
  if (!response.ok) throw new Error(task.error || response.status);
  output.textContent += JSON.stringify(task) + '\n';
  const es = new EventSource('/api/events?task_id=' + encodeURIComponent(task.id));
  es.addEventListener('ui', e => {
   const data = JSON.parse(e.data);
   output.textContent += e.data + '\n';
   if (['done','error'].includes(data.event.type)) es.close();
  });
  es.onerror = () => { output.textContent += 'Event stream disconnected; inspect task ' + task.id + '\n'; es.close(); };
 } catch (err) { output.textContent += String(err) + '\n'; }
};
</script></body></html>"#,
    )
}
