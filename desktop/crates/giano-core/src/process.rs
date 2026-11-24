//! External programs (git, Claude Code, the editor): finding them on the PATH and running
//! them without a console window, with a timeout and a cancel flag.

use std::env;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// A program found on the PATH (`claude` → `C:\...\claude.exe`), trying the Windows
/// extensions (`.exe`, `.cmd`, `.bat`) in that order.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &["exe", "cmd", "bat"] } else { &[""] };
    for dir in env::split_paths(&path) {
        for ext in exts {
            let candidate = if ext.is_empty() { dir.join(name) } else { dir.join(format!("{name}.{ext}")) };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// A command that opens no console window on Windows.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub struct Output {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run `cmd` to the end, writing `stdin` to it. Killed when `cancel` is set (error
/// "Cancelado.") or after `timeout`.
pub fn run(mut cmd: Command, stdin: Option<&[u8]>, cancel: Option<&AtomicBool>, timeout: Duration) -> Result<Output> {
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd.spawn().map_err(|e| Error::validation(format!("Não foi possível executar {program}: {e}")))?;
    let writer = match (stdin, child.stdin.take()) {
        (Some(bytes), Some(mut pipe)) => {
            let bytes = bytes.to_vec();
            // Closing the pipe (end of the thread) is the EOF the program waits for.
            Some(std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            }))
        }
        _ => None,
    };
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let cancelled = cancel.is_some_and(|c| c.load(Ordering::SeqCst));
        if cancelled || start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::validation(if cancelled {
                "Cancelado.".to_string()
            } else {
                format!("{} não terminou em {} min e foi interrompido.", file_name(&program), timeout.as_secs() / 60)
            }));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    Ok(Output {
        success: status.success(),
        code: status.code(),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

fn file_name(program: &str) -> String {
    Path::new(program).file_name().map_or(program.to_string(), |n| n.to_string_lossy().into_owned())
}
