#![deny(warnings)]
#![warn(clippy::all, clippy::nursery, clippy::pedantic)]

use std::env;
use std::fs::File;
use std::hint::unreachable_unchecked;
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
    NestingLimit(u64),
    ReloadState(Box<dyn Read>),
    Trace(String),
    Undef(String),
}

enum MacroValue {
    Text(String),
    BuiltinFunction(String),
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
            flags.push(Flag::ReloadState(Box::new(
                File::open(&reload_state).unwrap_or_else(|_| {
                    eprintln!("Couldn't open file {} for reading!", &arg);
                    process::exit(1);
                }),
            )));
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

fn read_int<I: Iterator<Item = u8>>(data: &mut I, sep: u8) -> i64 {
    let mut result: i64 = 0;
    let mut negative = false;
    for c in data {
        if c == sep {
            break;
        }
        if c == b'-' {
            negative = true;
        } else {
            result *= 10;
            result += i64::from(c - b'0');
        }
    }
    result * if negative { -1 } else { 1 }
}

fn print_to_diversion(cur_diversion: i64, content: &str, diversion_data: &mut Vec<String>) {
    if cur_diversion == 0 {
        print!("{}", content);
    } else if cur_diversion > 0 {
        let target = usize::try_from(cur_diversion - 1)
            .unwrap_or_else(|_| unsafe { unreachable_unchecked() });
        while diversion_data.len() <= target {
            diversion_data.push(String::new());
        }
        diversion_data[target].push_str(content);
    }
}

pub struct Delimiters {
    comment_start: u8,
    comment_end: u8,
    quote_start: u8,
    quote_end: u8,
}

impl Delimiters {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            comment_start: b'#',
            comment_end: b'\n',
            quote_start: b'`',
            quote_end: b'\'',
        }
    }
}

fn exec_file<F: Read>(
    file: &mut F,
    def_stack: &mut Vec<(String, MacroValue)>,
    cur_diversion: &mut i64,
    diversion_data: &mut Vec<String>,
    delimiters: &mut Delimiters,
) {
    let mut data: Vec<u8> = Vec::new();
    file.read_to_end(&mut data).unwrap_or_else(|e| {
        eprintln!("Couldn't read an input file: {}", e);
        process::exit(1);
    });
    let mut data = data.iter().copied();
    process_text(
        &mut data,
        def_stack,
        cur_diversion,
        diversion_data,
        delimiters,
    );
}

#[allow(dead_code)]
fn process_macro(
    cur_tok: &str,
    def_stack: &mut Vec<(String, MacroValue)>,
    cur_diversion: &mut i64,
    diversion_data: &mut Vec<String>,
    _delimiters: &mut Delimiters,
) {
    let mut matched = false;
    for def in def_stack.into_iter().rev() {
        if &def.0 == cur_tok {
            matched = true;
            eprintln!("Matched {}", def.0); // TODO
            break;
        }
    }
    if !matched {
        print_to_diversion(*cur_diversion, &cur_tok, diversion_data)
    }
}

fn skip_comment<I: Iterator<Item = u8>>(data: &mut I, end: u8) {
    for c in data {
        if c == end {
            break;
        }
    }
}

fn process_text<I: Iterator<Item = u8>>(
    data: &mut I,
    def_stack: &mut Vec<(String, MacroValue)>,
    cur_diversion: &mut i64,
    diversion_data: &mut Vec<String>,
    delimiters: &mut Delimiters,
) {
    let mut cur_tok = String::new();
    while let Some(c) = data.next() {
        match c {
            x if x == delimiters.comment_start => {
                process_macro(
                    &cur_tok,
                    def_stack,
                    cur_diversion,
                    diversion_data,
                    delimiters,
                );
                skip_comment(data, delimiters.comment_end);
            }
            b' ' | b'\t' | b'\r' | b'\n' => {
                process_macro(
                    &cur_tok,
                    def_stack,
                    cur_diversion,
                    diversion_data,
                    delimiters,
                );
                print_to_diversion(*cur_diversion, &(c as char).to_string()[..], diversion_data);
                cur_tok = String::new();
            }
            _ => cur_tok.push(c as char),
        }
    }
    process_macro(
        &cur_tok,
        def_stack,
        cur_diversion,
        diversion_data,
        delimiters,
    );
}

fn exec_reload_state<F: Read>(
    file: &mut F,
    def_stack: &mut Vec<(String, MacroValue)>,
    cur_diversion: &mut i64,
    diversion_data: &mut Vec<String>,
    delimiters: &mut Delimiters,
) {
    let mut data: Vec<u8> = Vec::new();
    file.read_to_end(&mut data).unwrap_or_else(|e| {
        eprintln!("Couldn't read a reload state file: {}", e);
        process::exit(1);
    });
    let mut data = data.iter().copied();
    while let Some(c) = data.next() {
        if c == delimiters.comment_start {
            skip_comment(&mut data, delimiters.comment_end);
        } else if c == b'C' {
            let start_len = read_int(&mut data, b',');
            let end_len = read_int(&mut data, b'\n');
            if start_len != 1 || end_len != 1 {
                eprintln!("Comment with multiple-character delimiters? Unheard of!");
                process::exit(1);
            }
            delimiters.comment_start = data.next().unwrap_or(b'#');
            delimiters.comment_end = data.next().unwrap_or(b'\n');
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after C declaration");
                process::exit(1);
            }
        } else if c == b'D' {
            let div_num = read_int(&mut data, b',');
            let content_len = read_int(&mut data, b'\n');
            let mut content = String::new();
            for _ in 0..content_len {
                content.push(data.next().unwrap_or(b'#') as char);
            }
            *cur_diversion = div_num;
            print_to_diversion(*cur_diversion, &content, diversion_data);
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after D declaration");
                process::exit(1);
            }
        } else if c == b'F' {
            let name_len = read_int(&mut data, b',');
            let value_len = read_int(&mut data, b'\n');
            let mut name = String::new();
            for _ in 0..name_len {
                name.push(data.next().unwrap_or(b'#') as char);
            }
            let mut value = String::new();
            for _ in 0..value_len {
                value.push(data.next().unwrap_or(b'#') as char);
            }
            def_stack.push((name, MacroValue::BuiltinFunction(value)));
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after T declaration");
                process::exit(1);
            }
        } else if c == b'Q' {
            let start_len = read_int(&mut data, b',');
            let end_len = read_int(&mut data, b'\n');
            if start_len != 1 || end_len != 1 {
                eprintln!("Quote with multiple-character delimiters? Unheard of!");
                process::exit(1);
            }
            delimiters.quote_start = data.next().unwrap_or(b'#');
            delimiters.quote_end = data.next().unwrap_or(b'\n');
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after Q declaration");
                process::exit(1);
            }
        } else if c == b'T' {
            let name_len = read_int(&mut data, b',');
            let value_len = read_int(&mut data, b'\n');
            let mut name = String::new();
            for _ in 0..name_len {
                name.push(data.next().unwrap_or(b'#') as char);
            }
            let mut value = String::new();
            for _ in 0..value_len {
                value.push(data.next().unwrap_or(b'#') as char);
            }
            def_stack.push((name, MacroValue::Text(value)));
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after T declaration");
                process::exit(1);
            }
        } else if c == b'V' {
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'1')) {
                eprintln!(
                    "Syntax error in reload state file: incorrect/missing version in V declaration"
                );
                process::exit(1);
            }
            let c = data.next();
            if matches!(c, None) || !matches!(c, Some(b'\n')) {
                eprintln!("Syntax error in reload state file: missing newline after V declaration");
                process::exit(1);
            }
        } else {
            print!("{}", c as char);
        }
    }
}

fn main() {
    let (prg_name, flags) = parse_args(env::args());
    let mut debug_flags = "aeq".into();
    let mut debug_out: Box<dyn Write> = Box::new(io::stderr());
    let mut nesting_limit: u64 = 1024;
    let mut traced = Vec::new();
    let mut def_stack = vec![(String::from("divert"), MacroValue::BuiltinFunction(String::from("divert")))];
    let mut cur_diversion = 0;
    let mut diversion_data = Vec::new();
    let mut delimiters = Delimiters::new();
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
            Flag::FatalWarning(_)
            | Flag::GnulyCorrect(_)
            | Flag::IncludePath(_)
            | Flag::Undef(_) => {} // We don't care yet
            Flag::File(mut x) => {
                exec_file(
                    &mut x,
                    &mut def_stack,
                    &mut cur_diversion,
                    &mut diversion_data,
                    &mut delimiters,
                );
            }
            Flag::NestingLimit(x) => nesting_limit = x,
            Flag::ReloadState(mut x) => {
                exec_reload_state(
                    &mut x,
                    &mut def_stack,
                    &mut cur_diversion,
                    &mut diversion_data,
                    &mut delimiters,
                );
            }
            Flag::Trace(x) => traced.push(x),
        }
    }
    drop(debug_flags);
    drop(debug_out);
    let _ = nesting_limit;
    drop(traced);
}
