/**
 * Copyright 2019 Benjamin Vaisvil
 */
extern crate num;
extern crate num_traits;

extern crate sysinfo;
#[macro_use]
extern crate num_derive;
#[macro_use]
extern crate log;


mod constants;
mod metrics;
mod render;
mod util;
mod zprocess;


use crate::render::TerminalRenderer;
use clap::{App, Arg};
use dirs;
use futures::executor::block_on;
use std::error::Error;
use std::fs;
use std::fs::{remove_file, File};
use std::io::{Write, stdout};
use std::panic;
use std::panic::PanicInfo;
use std::path::Path;
use std::process::exit;
use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::Terminal;
use env_logger;

fn panic_hook(info: &PanicInfo<'_>) {
    let location = info.location().unwrap(); // The current implementation always returns Some
    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => *s,
        None => match info.payload().downcast_ref::<String>() {
            Some(s) => &s[..],
            None => "Box<Any>",
        },
    };
    error!(
        "thread '<unnamed>' panicked at '{}', {}\r",
        msg,
        location);
    println!(
        "{}thread '<unnamed>' panicked at '{}', {}\r",
        termion::screen::ToMainScreen,
        msg,
        location
    );
}

fn init_terminal(){
    debug!("Initializing Terminal");
    let raw_term = stdout()
    .into_raw_mode()
    .expect("Could not bind to STDOUT in raw mode.");
    debug!("Create Mouse Term");
    let mouse_term = MouseTerminal::from(raw_term);
    debug!("Create Alternate Screen");
    let mut screen = AlternateScreen::from(mouse_term);
    debug!("Clear Screen");
    // Need to clear screen for TTYs
    write!(screen, "{}", termion::clear::All).expect("Attempt to write to alternate screen failed.");
}

fn restore_terminal(){
    debug!("Restoring Terminal");
    let raw_term = stdout()
        .into_raw_mode()
        .expect("Could not bind to STDOUT in raw mode.");
    //let stdout = MouseTerminal::from(stdout);
    let mut screen = AlternateScreen::from(raw_term);
    // Restore cursor position and clear screen for TTYs
    write!(screen, "{}{}", termion::cursor::Goto(1,1), termion::clear::All).expect("Attempt to write to alternate screen failed.");
    let backend = TermionBackend::new(screen);
    let mut terminal = Terminal::new(backend).expect("Could not create new terminal.");
    terminal.show_cursor().expect("Restore cursor failed.");
}

fn start_zenith(
    rate: u64,
    cpu_height: u16,
    net_height: u16,
    disk_height: u16,
    process_height: u16,
    sensor_height: u16,
    disable_history: bool,
    db_path: &str,
) -> Result<(), Box<dyn Error>> {

    debug!("Starting with Arguments: rate: {}, cpu: {}, net: {}, disk: {}, process: {}, disable_history: {}, db_path: {}",
          rate,
          cpu_height,
          net_height,
          disk_height,
          process_height,
          disable_history,
          db_path
    );

    //check lock
    let lock_path = Path::new(db_path).join(Path::new(".zenith.lock"));
    let db = if lock_path.exists() {
        debug!("Lock exists.");
        if !disable_history {
            print!("{:} exists and history recording is on. Is another copy of zenith open? If not remove the path and open zenith again.", lock_path.display());
            exit(1);
        } else {
            None
        }
    } else {
        if !disable_history {
            
            let p = Path::new(db_path);
            if !p.exists() {
                debug!("Creating DB dir.");
                fs::create_dir(p).expect("Couldn't Create DB dir.");
            }
            debug!("Creating Lock");
            File::create(&lock_path)?;
            Some(p.to_path_buf())
        } else {
            None
        }
    };

    init_terminal();

    // setup a panic hook so we can see our panic messages.
    panic::set_hook(Box::new(|info| {
        panic_hook(info);
    }));

    debug!("Create Renderer");
    let mut r = TerminalRenderer::new(
        rate,
        cpu_height as i16,
        net_height as i16,
        disk_height as i16,
        process_height as i16,
        sensor_height as i16,
        db,
    );

    let z = block_on(r.start());
    
    debug!("Shutting Down.");
    if !disable_history && lock_path.exists() {
        debug!("Removing Lock");
        remove_file(lock_path)?
    }

    restore_terminal();

    Ok(z)
}

fn validate_refresh_rate(arg: String) -> Result<(), String> {
    let val = arg.parse::<u64>().unwrap_or(0);
    if val >= 1000 {
        Ok(())
    } else {
        Err(format!(
            "{} Enter a refresh rate that is at least 1000 ms",
            &*arg
        ))
    }
}

fn validate_height(arg: String) -> Result<(), String> {
    let val = arg.parse::<i64>().unwrap_or(0);
    if val >= 0 {
        Ok(())
    } else {
        Err(format!(
            "{} Enter a height greater than or equal to 0.",
            &*arg
        ))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let default_db_path = dirs::cache_dir()
        .unwrap_or(Path::new("./").to_owned())
        .join("zenith");
    let default_db_path = default_db_path
        .to_str()
        .expect("Couldn't set default db path");
    let matches = App::new("zenith")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Benjamin Vaisvil <ben@neuon.com>")
        .about(
            "Zenith, sort of like top but with histograms.
Up/down arrow keys move around the process table. Return (enter) will focus on a process.
Tab switches the active section. Active sections can be expanded (e) and minimized (m).
Using this you can create the layout you want.",
        )
        .arg(
            Arg::with_name("refresh-rate")
                .short("r")
                .long("refresh-rate")
                .value_name("INT")
                .default_value("2000")
                .validator(validate_refresh_rate)
                .help(format!("Refresh rate in milliseconds.").as_str())
                .takes_value(true),
        )
        .arg(
            Arg::with_name("cpu-height")
                .short("c")
                .long("cpu-height")
                .value_name("INT")
                .default_value("10")
                .validator(validate_height)
                .help(format!("Height of CPU/Memory visualization.").as_str())
                .takes_value(true),
        )
        .arg(
            Arg::with_name("net-height")
                .short("n")
                .long("net-height")
                .value_name("INT")
                .default_value("10")
                .validator(validate_height)
                .help(format!("Height of Network visualization.").as_str())
                .takes_value(true),
        )
        .arg(
            Arg::with_name("disk-height")
                .short("d")
                .long("disk-height")
                .value_name("INT")
                .default_value("10")
                .validator(validate_height)
                .help(format!("Height of Disk visualization.").as_str())
                .takes_value(true),
        )
        // .arg(
        //     Arg::with_name("sensor-height")
        //         .short("s")
        //         .long("sensor-height")
        //         .value_name("INT")
        //         .default_value("10")
        //         .validator(validate_height)
        //         .help(format!("Height of Sensor visualization.").as_str())
        //         .takes_value(true),
        // )
        .arg(
            Arg::with_name("process-height")
                .short("p")
                .long("process-height")
                .value_name("INT")
                .default_value("8")
                .validator(validate_height)
                .help(format!("Min Height of Process Table.").as_str())
                .takes_value(true),
        )
        .arg(
            Arg::with_name("disable-history")
                .long("disable-history")
                .help(format!("Disables history when flag is present").as_str())
                .takes_value(false),
        )
        .arg(
            Arg::with_name("db")
                .long("db")
                .value_name("STRING")
                .default_value(default_db_path)
                .help(format!("Database to use, if any.").as_str())
                .takes_value(true),
        )
        .get_matches();

    env_logger::init();
    info!("Starting zenith {}", env!("CARGO_PKG_VERSION"));

    start_zenith(
        matches
            .value_of("refresh-rate")
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        matches
            .value_of("cpu-height")
            .unwrap()
            .parse::<u16>()
            .unwrap(),
        matches
            .value_of("net-height")
            .unwrap()
            .parse::<u16>()
            .unwrap(),
        matches
            .value_of("disk-height")
            .unwrap()
            .parse::<u16>()
            .unwrap(),
        matches
            .value_of("process-height")
            .unwrap()
            .parse::<u16>()
            .unwrap(),
        0,
        matches.is_present("disable-history"),
        matches.value_of("db").unwrap(),
    )
}
