use std::process::Command;

use anyhow::Ok;
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
        let args2 = args[3..].to_vec();
        debug!("{:?}", args2);
        Command::new(&args[2])
            .args(&args2)
            .spawn()
            .expect("failed to run command");
    } else {
        anyhow::bail!("invalid command");
    }
    Ok(())
}
