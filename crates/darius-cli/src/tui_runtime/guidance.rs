use super::actor::SessionActor;
use crate::runtime::SessionRuntime;
impl SessionActor {
    pub fn guidance(&self, runtime: &SessionRuntime) -> bool {
        if runtime.is_setup() {
            self.status(
                "Setup required: set DARIUS_API_KEY or OPENAI_API_KEY, then run config init.",
            );
            self.status("No goal was run; no completion was claimed.");
            return true;
        }
        if runtime.is_offline_demo() {
            self.status("Runtime state: offline-demo");
            self.status("Offline demo: no real file analysis or completion was performed.");
            return true;
        }
        false
    }
}
