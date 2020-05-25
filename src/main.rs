//! Run sequential commands with basic pipelining syntax similar to sh.
//! Designed to be portable and simple for the 80% case: running a
//! command with arguments and combining commands through pipes.
//!
//! Note: no pipe redirection, no failure handling, nothing fancy.
//! Will add features as I need them in my workflow, rather than trying to
//! support the universe.
//!
//! TODO:
//! - Verbosity flag.
//! - Colorize comments and command literals.
//! - Graceful errors (no panic!), panicking is bad user experience.
//! - Support Serde on top of "custom" format?
//! - Shell interface (basically, a loop with a prompt).
//!

mod config;
mod env;
mod parser;
mod pipeline;
mod util;

use config::Config;
use env::Environment;
use parser::{Item, ItemParser};
use pipeline::Pipeline;
use std;
use std::fs::File;
use std::io::prelude::*;

fn main() {
    // TODO(jfm): Handle multiple ".run" files.
    // Do we want to execute them all? Probably not? Should there be more than
    // one? Not sure. TBD.

    let mut file: String = String::new();
    let mut args = std::env::args().skip(1).peekable();

    if let Some(mut run_file) = args.next() {
        if !run_file.ends_with(".run") {
            run_file.push_str(".run");
        }
        File::open(&run_file)
            .map_err(|e| format!("opening {}: {}", &run_file, e))
            .unwrap()
            .read_to_string(&mut file)
            .expect("reading run file");
    }

    // Consume any config flags we care about.
    let config = Config::from_args(&mut args);

    // Wrap each unique argument in quotes for the environment parser.
    // Quotes get stripped on entry, so we add them back.
    // #perf
    let s: String = args.fold(String::new(), |mut buf, next| {
        buf.push('"');
        buf.extend(next.chars());
        buf.push('"');
        buf.push(' ');
        buf
    });

    let environment: Environment = s
        .parse()
        .map_err(|e| format!("parsing environment: {}", e))
        .unwrap();

    let items = ItemParser { env: &environment }
        .parse(&file)
        .map_err(|e| format!("parsing commands: {}", e))
        .unwrap();

    if config.dry_run {
        for item in items {
            match item {
                Item::Comment(comment) => {
                    println!("{}", comment);
                }
                Item::Pipeline { cmds, terminus, .. } => {
                    for cmd in cmds {
                        println!("{}", &cmd);
                    }
                    if let Some(terminus) = terminus {
                        println!("> {}", &terminus.to_string_lossy());
                    }
                }
            };
        }
    } else {
        for item in items {
            match item {
                Item::Comment(comment) => {
                    println!("{}", comment);
                }
                Item::Pipeline { ignore_failure, .. } => {
                    if let Err(err) = item.execute(std::io::stdout()) {
                        println!("error: {}", err);

                        if !ignore_failure {
                            break;
                        }
                    }
                }
            }
        }
    }
}
