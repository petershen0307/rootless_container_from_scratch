use std::ffi::CString;
use std::io::Write;
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;
use std::{env, fs, path, thread};

use anyhow::Context;
use nix::sched::CloneFlags;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::{Gid, Pid, Uid};
use nix::{mount, sched, sys, unistd};
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
    let (read_pipe, write_pipe) = unistd::pipe().expect("create pipe failed");
    let child_pid = clone(args, &fs_root, read_pipe).expect("fork failed");
    info!("child pid={}", child_pid);
    adjust_uid_map(child_pid, unistd::getuid())?;
    // adjust_gid_map(child_pid, unistd::getgid())?;
    unistd::write(write_pipe, "go".as_bytes()).expect("write pipe failed");
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

fn adjust_uid_map(pid: Pid, host_uid: Uid) -> anyhow::Result<()> {
    let uid_map_path = format!("/proc/{pid}/uid_map");
    info!("uid_map={uid_map_path}");
    let mut uid_map_file = fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&uid_map_path)?;
    // https://man7.org/linux/man-pages/man7/user_namespaces.7.html
    // content {the uid for pid's namespace} {host uid} {uid length}
    let content = format!("0 {host_uid} 1");
    uid_map_file
        .write_all(content.as_bytes())
        .context("write uid failed")?;
    Ok(())
}

fn adjust_gid_map(pid: Pid, host_gid: Gid) -> anyhow::Result<()> {
    let gid_map_path = format!("/proc/{pid}/gid_map");
    info!("gid_map={gid_map_path}");
    let mut gid_map_file = fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&gid_map_path)?;
    // https://man7.org/linux/man-pages/man7/user_namespaces.7.html
    // content {the gid for pid's namespace} {host gid} {gid length}
    let content = format!("0 {host_gid} 1");
    gid_map_file
        .write_all(content.as_bytes())
        .context("write gid failed")?;
    Ok(())
}

fn clone(args: &[String], fs_root: &str, read_pipe: OwnedFd) -> anyhow::Result<Pid> {
    const STACK_SIZE: usize = 1024 * 1024;
    let stack: &mut [u8; STACK_SIZE] = &mut [0; STACK_SIZE];
    let child_pid = unsafe {
        sched::clone(
            Box::new(|| -> isize {
                let mut buf = [0; 1024];
                unistd::read(read_pipe.as_raw_fd(), &mut buf).expect("read pipe failed");
                // setup root dir
                unistd::chroot(fs_root).expect("set root dir failed");
                // setup current directory to root
                unistd::chdir("/").expect("set current dir failed");
                // setup hostname
                unistd::sethostname("container").expect("set hostname failed");
                // mount proc
                mount::mount(
                    Some("proc"),
                    "proc",
                    Some("proc"),
                    mount::MsFlags::empty(),
                    Option::<&'static [u8]>::None,
                )
                .expect("mount failed");
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
        .context("can't clone process")?
    };
    Ok(child_pid)
}

fn fork(args: &[String], fs_root: &str, read_pipe: OwnedFd) -> anyhow::Result<Pid> {
    let child_pid;
    unsafe {
        let fork_result = unistd::fork().expect("fork failed");

        child_pid = match fork_result {
            unistd::ForkResult::Parent { child } => child,
            _ => {
                let mut buf = [0; 1024];
                unistd::read(read_pipe.as_raw_fd(), &mut buf).expect("read pipe failed");
                sched::unshare(CloneFlags::CLONE_NEWUSER).expect("unshare user namespace failed");
                sched::unshare(CloneFlags::CLONE_NEWPID).expect("unshare pid namespace failed");
                sched::unshare(CloneFlags::CLONE_NEWNS).expect("unshare mount namespace failed");
                sched::unshare(CloneFlags::CLONE_NEWUTS).expect("unshare uts namespace failed");
                // setup root dir
                unistd::chroot(fs_root).expect("set root dir failed");
                // setup current directory to root
                unistd::chdir("/").expect("set current dir failed");
                // setup hostname
                unistd::sethostname("container").expect("set hostname failed");
                // mount proc
                mount::mount(
                    Some("proc"),
                    "proc",
                    Some("proc"),
                    mount::MsFlags::empty(),
                    Option::<&'static [u8]>::None,
                )
                .expect("mount failed");
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
                Pid::from_raw(0)
            }
        }
    }
    Ok(child_pid)
}
