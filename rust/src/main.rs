use std::ffi::CString;
use std::process::{self};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Ok};
use nix::sched::CloneFlags;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use nix::{sched, sys, unistd};
use tracing::{debug, info, Level};

// prun run <command> <args>
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_level(true)
        .with_max_level(Level::TRACE)
        .init();
    let args = std::env::args().collect::<Vec<String>>();
    anyhow::ensure!(args.len() > 1, "incorrect parameter");
    info!(
        "pid={}, user id={}, hostname={}",
        unistd::getpid(),
        unistd::getuid(),
        unistd::gethostname()
            .expect("get hostname")
            .into_string()
            .expect("convert OsString to String")
    );
    match args[1].to_lowercase().as_str() {
        "run" => {
            run(&args[2..])?;
        }
        "exec" => {
            exec(&args[2..])?;
        }
        _ => {
            anyhow::bail!("invalid command");
        }
    }
    Ok(())
}

fn exec(args: &[String]) -> anyhow::Result<()> {
    let c_string_command = CString::new(args[0].clone()).unwrap();
    // https://man7.org/linux/man-pages/man3/exec.3.html
    // The first argument, by convention, should point to the filename associated with the file being executed.
    let args2 = args[0..].to_vec();
    let mut args2_c_string = Vec::new();
    for arg in args2 {
        args2_c_string.push(CString::new(arg).unwrap());
    }
    let mut args2_c_str = Vec::new();
    for arg in args2_c_string.iter() {
        args2_c_str.push(arg.as_c_str());
    }
    unistd::execv(c_string_command.as_c_str(), &args2_c_str).expect("exec got error");
    Ok(())
}

fn run(args: &[String]) -> anyhow::Result<()> {
    const STACK_SIZE: usize = 1024 * 1024;
    let stack: &mut [u8; STACK_SIZE] = &mut [0; STACK_SIZE];
    unsafe {
        sched::clone(
            Box::new(|| -> isize {
                unistd::sethostname("container").expect("set hostname failed");
                info!(
                    "sched::clone pid={}, user id={}, hostname={}",
                    unistd::getpid(),
                    unistd::getuid(),
                    unistd::gethostname()
                        .expect("get hostname")
                        .into_string()
                        .expect("convert OsString to String")
                );
                exec(args).unwrap();
                0
            }),
            stack,
            CloneFlags::CLONE_NEWUSER
                | CloneFlags::CLONE_NEWUTS
                | CloneFlags::CLONE_NEWPID
                | CloneFlags::CLONE_NEWNS,
            Some(Signal::SIGCHLD as i32),
        )
        .context("can't clone process")?;
    }
    // wait all children process stop
    while let WaitStatus::StillAlive =
        sys::wait::waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG))
            .context("wait child process failed")?
    {
        thread::sleep(Duration::from_secs(1));
    }
    info!("leave");
    Ok(())
}
