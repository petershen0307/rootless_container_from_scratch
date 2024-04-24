use std::ffi::CString;
use std::time::Duration;
use std::{env, path, thread};

use anyhow::Context;
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
    anyhow::ensure!(args.len() > 1, "prun [run|exec]");
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
            anyhow::ensure!(args.len() > 3, "prun run <image> <command> <args>");
            run(&args[2], &args[3..])?;
        }
        "exec" => {
            anyhow::ensure!(args.len() > 2, "prun exec <command> <args>");
            exec(&args[2..])?;
        }
        _ => {
            anyhow::bail!("invalid command");
        }
    }
    Ok(())
}

fn exec(args: &[String]) -> anyhow::Result<()> {
    // https://man7.org/linux/man-pages/man3/exec.3.html
    // The first argument, by convention, should point to the filename associated with the file being executed.
    let args: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();
    unistd::execvp(&args[0], &args).expect("exec got error");
    Ok(())
}

fn run(image: &String, args: &[String]) -> anyhow::Result<()> {
    let fs_root = resolve_image_path(image)?;
    debug!("fs_root={}", fs_root);
    const STACK_SIZE: usize = 1024 * 1024;
    let stack: &mut [u8; STACK_SIZE] = &mut [0; STACK_SIZE];
    unsafe {
        sched::clone(
            Box::new(|| -> isize {
                // setup root dir
                unistd::chroot(fs_root.as_str()).expect("set root dir failed");
                // setup current directory to root
                unistd::chdir("/").expect("set current dir failed");
                // setup hostname
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
                // execute process
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

fn resolve_image_path(image: &String) -> anyhow::Result<String> {
    let fs_root = env::var("FS_ROOT").unwrap_or("/home/peter/filesystem".to_string());
    if let Ok(fs_root) = path::Path::new(&fs_root)
        .join(image)
        .into_os_string()
        .into_string()
    {
        Ok(fs_root)
    } else {
        anyhow::bail!("convert osstring to string failed")
    }
}
