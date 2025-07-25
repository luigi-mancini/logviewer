mod command_handler;
mod controller;
mod log_file;
mod log_viewer;

use anyhow::Result;
use env_logger::{Builder, Target};
use log::{info, LevelFilter};
use std::panic;
use crossterm::terminal::disable_raw_mode;

use clap::Parser;
use std::path::PathBuf; // We'll use PathBuf for safer file handling

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "This application allows you to view the content of a log file.")]
struct Cli {
    filename: PathBuf, 
}

fn main() -> Result<()> {
    let args= Cli::parse();

    let target = Box::new(std::fs::File::create("app.log").unwrap());

    Builder::new()
        .target(Target::Pipe(target))
        .filter_level(LevelFilter::Debug) // Set level here
        .init();

    panic::set_hook(Box::new(|panic_info| {

        let _ret = disable_raw_mode();

        if let Some(location) = panic_info.location() {
            eprintln!(
                "Panic occurred in file '{}' at line {}",
                location.file(),
                location.line()
            );
        } else {
            eprintln!("Panic occurred but can't get location information...");
        }
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("Panic message: {:?}", s);
        } else {
            eprintln!("Panic payload is not a string.");
        }
    }));

    info!("Starting log viewer application");
    if let Some(path) = args.filename.to_str() {
        let mut controller = controller::Controller::new(path)?;
        controller.run()?;
    } else {
        eprintln!("Invalid file path provided.");
        return Err(anyhow::anyhow!("Invalid file path"));
    }

    Ok(())
}
