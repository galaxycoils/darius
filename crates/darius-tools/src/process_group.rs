//! Process-group runner: own session, poll, kill, reap.
use crate::execution::{ExecutionContext, RunEnd, RunResult};
use std::path::Path;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(25);

#[cfg(unix)]
pub(crate) fn run_process_group(cmd: &str, cwd: &Path, ctx: &ExecutionContext) -> RunResult {
    use std::os::unix::process::CommandExt;
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let out_path = tmp.path().join("o");
    let err_path = tmp.path().join("e");
    let mut spawn = std::process::Command::new("sh");
    spawn.arg("-c").arg(cmd).current_dir(cwd);
    spawn.stdout(std::fs::File::create(&out_path).map_err(|e| e.to_string())?);
    spawn.stderr(std::fs::File::create(&err_path).map_err(|e| e.to_string())?);
    unsafe {
        spawn.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        })
    };
    let mut child = spawn.spawn().map_err(|e| e.to_string())?;
    let outputs = || {
        let o = std::fs::read(&out_path).unwrap_or_default();
        (o, std::fs::read(&err_path).unwrap_or_default())
    };
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let (o, e) = outputs();
            return Ok(RunEnd::Done(status.code(), o, e));
        }
        if ctx.cancel.is_cancelled() || Instant::now() >= ctx.deadline {
            let rc = unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };
            let gone =
                rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            let status = child.wait().map_err(|e| e.to_string())?;
            if gone {
                let (o, e) = outputs();
                return Ok(RunEnd::Done(status.code(), o, e));
            }
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
