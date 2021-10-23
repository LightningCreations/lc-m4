#![deny(warnings)]
#![warn(clippy::all, clippy::nursery, clippy::pedantic)]

use std::env;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::mem::drop;
use std::process;
use std::vec::Vec;

fn help() {
    println!("We support reload-state. That's what you care about autom4te, right?");
}

enum Flag {
    DebugFile(String),
    DebugFlags(String),
    FatalWarning(bool),
    File(Box<dyn Read>),
    GnulyCorrect(bool),
    IncludePath(String),
    NestingLimit(u32),
    ReloadState(Box<dyn Read>),
    Trace(String),
    Undef(String),
}

fn parse_args<I: Iterator<Item = String>>(mut args: I) -> (String, Vec<Flag>) {
    let prg_name = args.next().unwrap_or_else(|| "m4".into()); // If we were (erroneously) not handed a program name, gracefully handle it
    let mut flags: Vec<Flag> = Vec::new();
    let mut any_files = false;
    for arg in args {
        eprintln!("{}", arg);
        if arg == "--help" {
            help();
            process::exit(0);
        } else if arg == "--fatal-warning" {
            flags.push(Flag::FatalWarning(true));
        } else if arg == "--gnu" {
            flags.push(Flag::GnulyCorrect(true));
        } else if arg == "--traditional" {
            flags.push(Flag::GnulyCorrect(false));
        } else if let Some(debug_flags) = arg.strip_prefix("--debug=") {
            flags.push(Flag::DebugFlags(debug_flags.into()));
        } else if let Some(debug_file) = arg.strip_prefix("--debugfile=") {
            flags.push(Flag::DebugFile(debug_file.into()));
        } else if let Some(include_path) = arg.strip_prefix("--include=") {
            flags.push(Flag::IncludePath(include_path.into()));
        } else if let Some(nesting_limit) = arg.strip_prefix("--nesting-limit=") {
            flags.push(Flag::NestingLimit(nesting_limit.parse().unwrap_or_else(
                |_| {
                    eprintln!("lc-m4: Nesting limit must be a number");
                    process::exit(1);
                },
            )));
        } else if let Some(reload_state) = arg.strip_prefix("--reload-state=") {
            flags.push(Flag::ReloadState(Box::new(File::open(&reload_state).unwrap_or_else(
                |_| {
                    eprintln!("Couldn't open file {} for reading!", &arg);
                    process::exit(1);
                },
            ))));
        } else if let Some(traced) = arg.strip_prefix("--trace=") {
            flags.push(Flag::Trace(traced.into()));
        } else if let Some(undef) = arg.strip_prefix("--undefine=") {
            flags.push(Flag::Undef(undef.into()));
        } else if arg == "-" {
            any_files = true;
            flags.push(Flag::File(Box::new(io::stdin())));
        } else if arg.starts_with('-') {
            eprintln!("Unrecognized arg: {}", arg);
            process::exit(1);
        } else {
            any_files = true;
            flags.push(Flag::File(Box::new(File::open(&arg).unwrap_or_else(
                |_| {
                    eprintln!("Couldn't open file {} for reading!", &arg);
                    process::exit(1);
                },
            ))));
        }
    }
    if !any_files {
        flags.push(Flag::File(Box::new(io::stdin())));
    }
    (prg_name, flags)
}

fn read_int<F: Read>(data: &mut Iterator<Item = u8>, sep: u8) {

}

fn exec_file<F: Read>(file: &mut F) {
    let mut data: String = "".into();
    file.read_to_string(&mut data).unwrap_or_else(|e| {
        eprintln!("Couldn't read a file: {}", e);
        process::exit(1);
    });
    eprintln!("{}", data);
}

fn exec_reload_state<F: Read>(file: &mut F) {
    let mut data: Vec<u8> = Vec::new();
    file.read_to_end(&mut data).unwrap_or_else(|e| {
        eprintln!("Couldn't read a reload state file: {}", e);
        process::exit(1);
    });
    let mut comment_start: u8 = b'#';
    let mut comment_end: u8 = b'\n';
    let mut data = data.iter();
    while let Some(&c) = data.next() {
        if c == comment_start {
            while let Some(&c) = data.next() {
                if c == comment_end { break; }
            }
        } else if c == b'C' {
            let slen = read_int(data, ',');
            let elen = read_int(data, '\n');
            if slen != 1 || elen != 1 {
                println!("Comment with multiple-character delimiters? Unheard of!");
                process::exit(1);
            }
        } else {
            print!("{}", c);
        }
    }
}

fn main() {
    let (prg_name, flags) = parse_args(env::args());
    let mut debug_flags = "aeq".into();
    let mut debug_out: Box<dyn Write> = Box::new(io::stderr());
    let mut nesting_limit: u32 = 1024;
    let mut traced = Vec::new();
    for f in flags {
        match f {
            Flag::DebugFile(x) => {
                debug_out = match File::create(x) {
                    io::Result::Ok(x) => Box::new(x),
                    io::Result::Err(x) => {
                        eprintln!("{}: Error creating debug file: {}", prg_name, x);
                        process::exit(1)
                    }
                }
            }
            Flag::DebugFlags(x) => debug_flags = x,
            Flag::FatalWarning(_) => {} // We don't care yet
            Flag::File(mut x) => {
                exec_file(&mut x);
            }
            Flag::GnulyCorrect(_) => {} // We don't care yet
            Flag::IncludePath(_) => {} // We don't care yet
            Flag::NestingLimit(x) => nesting_limit = x,
            Flag::ReloadState(mut x) => {
                exec_reload_state(&mut x);
            }
            Flag::Trace(x) => traced.push(x),
            Flag::Undef(_) => {}
        }
    }
    drop(debug_flags);
    drop(debug_out);
    let _ = nesting_limit;
    drop(traced);
}
