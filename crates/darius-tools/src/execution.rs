//! Cancellable execution context, executor contract, process runner.
use std::path::Path;
use std::time::{Duration, Instant};

pub struct ExecutionContext {
    pub cancel: tokio_util::sync::CancellationToken,
    pub deadline: Instant,
}
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: &crate::ToolCall, ctx: &ExecutionContext) -> crate::ToolOutcome;
}
pub(crate) enum RunEnd {
    Done(Option<i32>, Vec<u8>, Vec<u8>),
    Interrupted,
    TimedOut,
}
type RunResult = Result<RunEnd, String>;
const POLL: Duration = Duration::from_millis(25);
#[cfg(unix)]
pub(crate) fn run_process_group(cmd: &str, cwd: &Path, ctx: &ExecutionContext) -> RunResult {
    use std::os::unix::process::CommandExt;
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut spawn = std::process::Command::new("sh");
    spawn.arg("-c").arg(cmd).current_dir(cwd);
    spawn.stdout(std::fs::File::create(tmp.path().join("o")).map_err(|e| e.to_string())?);
    spawn.stderr(std::fs::File::create(tmp.path().join("e")).map_err(|e| e.to_string())?);
    unsafe {
        spawn.pre_exec(|| {
            libc::setsid();
            Ok(())
        })
    };
    let mut child = spawn.spawn().map_err(|e| e.to_string())?;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let o = std::fs::read(tmp.path().join("o")).unwrap_or_default();
            let e = std::fs::read(tmp.path().join("e")).unwrap_or_default();
            return Ok(RunEnd::Done(status.code(), o, e));
        }
        if ctx.cancel.is_cancelled() || Instant::now() >= ctx.deadline {
            unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };
            let _ = child.wait();
            if ctx.cancel.is_cancelled() {
                return Ok(RunEnd::Interrupted);
            }
            return Ok(RunEnd::TimedOut);
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(not(unix))]
pub(crate) fn run_process_group(_cmd: &str, _cwd: &Path, _ctx: &ExecutionContext) -> RunResult {
    Err("shell unavailable on this platform".into())
}
