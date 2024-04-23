use std::{
    io::stdout,
    process::{Command, Stdio},
};

use anyhow::Context;
use nix::sched;
use nix::sched::CloneFlags;
use nix::sys::signal::Signal;
use tracing::{debug, info, Level};

// prun run <command> <args>
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();
    let args = std::env::args().collect::<Vec<String>>();
    anyhow::ensure!(args.len() > 1, "incorrect parameter");
    info!("args len:{}", args.len());
    if args[1].to_lowercase().as_str() == "run" {
        const STACK_SIZE: usize = 1024 * 1024;
        let stack: &mut [u8; STACK_SIZE] = &mut [0; STACK_SIZE];
        unsafe {
            sched::clone(
                Box::new(|| -> isize {
                    let args2 = args[3..].to_vec();
                    debug!("{:?}", args2);
                    let mut child = Command::new(&args[2])
                        .args(&args2)
                        .spawn()
                        .expect("failed to run command");
                    child.wait().expect("failed to wait child output");
                    0
                }),
                stack,
                CloneFlags::CLONE_NEWUSER
                    | CloneFlags::CLONE_NEWUTS
                    | CloneFlags::CLONE_NEWPID
                    | CloneFlags::CLONE_NEWNS,
                Some(Signal::SIGCHLD as i32),
            )
            .expect("can't run process");
        }
    } else {
        anyhow::bail!("invalid command");
    }
    Ok(())
}
