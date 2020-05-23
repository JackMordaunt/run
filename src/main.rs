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
mod util;

use config::Config;
use env::Environment;
use glob::glob;
use parser::{Cmd, Item, ItemParser};
use std;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::process::{Child, Command, Stdio};

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

    if cfg!(debug_assertions) {
        dbg!(&config);
    }

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

    if cfg!(debug_assertions) {
        dbg!(&s);
    }

    let environment: Environment = s
        .parse()
        .map_err(|e| format!("parsing environment: {}", e))
        .unwrap();

    if cfg!(debug_assertions) {
        dbg!(&environment);
    }

    let items = ItemParser { env: &environment }
        .parse(&file)
        .map_err(|e| format!("parsing commands: {}", e))
        .unwrap();

    if cfg!(debug_assertions) {
        dbg!(&items);
    }

    if config.dry_run {
        for item in items {
            match item {
                Item::Comment(comment) => {
                    println!("{}", comment);
                }
                Item::Pipeline {
                    cmds,
                    literal: _literal,
                } => {
                    for cmd in cmds {
                        println!("{}", &cmd);
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
                Item::Pipeline {
                    cmds,
                    literal: _literal,
                } => {
                    let mut prev = None;
                    let mut cmds = cmds.iter().peekable();
                    while let Some(cmd) = cmds.next() {
                        println!("{}", &cmd);
                        let Cmd { name, args } = cmd;
                        // Note(jfm):
                        //  Should builtins get access to pipes? Do they need them?
                        //  Should we check to see if an "rm" utility exists on the machine?
                        //  User would probably like to use their installed rm utitliy.
                        match name.as_ref() {
                            "rm" => {
                                if let Err(err) = args
                                    .iter()
                                    .map(|arg| rm(arg))
                                    .collect::<Result<Vec<_>, _>>()
                                {
                                    panic!("rm {}: {}", args.join(" "), err);
                                }
                            }
                            _ => {
                                let stdin = prev.map_or(Stdio::inherit(), |output: Child| {
                                    Stdio::from(output.stdout.unwrap())
                                });
                                let stdout = if cmds.peek().is_some() {
                                    Stdio::piped()
                                } else {
                                    Stdio::inherit()
                                };
                                let output = Command::new(name)
                                    .current_dir(
                                        std::env::current_dir()
                                            .expect("fetching current working directory"),
                                    )
                                    .args(args)
                                    .stdin(stdin)
                                    .stdout(stdout)
                                    .spawn();
                                match output {
                                    Ok(output) => prev = Some(output),
                                    Err(err) => {
                                        panic!("{}: {}", name, err);
                                    }
                                };
                            }
                        };
                    }
                    if let Some(mut finish) = prev {
                        finish.wait().ok();
                    }
                }
            }
        }
    }
}

// rm the given glob pattern.
// Does what you expect: removes the files that match the pattern.
//
// TODO: Handle powershell path expansions eg
//  "$Env:UserProfile" -> C:\Users\<user>
//
fn rm(pattern: &str) -> Result<(), Box<dyn Error>> {
    glob(pattern)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| std::fs::remove_file(&entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}
