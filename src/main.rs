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

use glob::glob;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

fn main() {
    // TODO(jfm): Handle multiple ".run" files.
    // Do we want to execute them all? Probably not? Should there be more than
    // one? Not sure. TBD.

    let mut file: String = String::new();
    let mut args = env::args().skip(1).peekable();

    // Note: If no ".run" is specified, then we rely on glob to pick the first
    // one it sees. May not be deterministic... Is it worth the convenience?
    if let Some(first) = args.peek() {
        let run_file = if !first.ends_with(".run") {
            glob("*.run")
                .expect("bad glob pattern")
                .next()
                .expect("no entries")
                .expect("path error")
        } else {
            match args.next() {
                Some(arg) => arg.into(),
                None => panic!("no script name provided"),
            }
        };
        File::open(run_file)
            .expect("opening run file")
            .read_to_string(&mut file)
            .expect("reading run file");
    }

    let environment: Environment = args
        .fold(String::new(), |mut buf, next| {
            buf.extend(next.chars());
            buf
        })
        .parse()
        .map_err(|e| format!("parsing: {}", e))
        .unwrap();

    // Parsing is done very simple, line-wise then pipe-wise.
    //
    // Single command on a line:
    //  command arg\n
    //
    // Multiple commands piped:
    // Obviously, stdout of each command gets piped into stdin of the next.
    //
    //  command arg | command arg | command arg\n
    //  ^----------^ ^-----------^ ^------------^

    let items = file.lines().filter_map(|line| {
        if line.starts_with("//") {
            Some(Item::Comment(line.to_owned()))
        } else {
            Some(Item::Pipeline {
                literal: line.to_owned(),
                cmds: line
                    .split("|")
                    .filter_map(|cmd| {
                        let mut words = cmd.split_whitespace();
                        if let Some(name) = words.next() {
                            Some(Cmd {
                                name: name.to_owned(),
                                args: words
                                    .map(String::from)
                                    .map(|arg| {
                                        // TODO(jfm): Proper error handling.

                                        // Basically, if arg is "$<numeric>" we parse
                                        // the number and lookup the corresponding positional argument.
                                        // If arg is "$<identifier>" we lookup the named argument.
                                        // If either one doesn't exist we throw up an error.

                                        if arg.starts_with('$') {
                                            if let Ok(index) = arg
                                                .chars()
                                                .skip(1)
                                                .next()
                                                .unwrap()
                                                .to_string()
                                                .parse::<usize>()
                                            {
                                                match environment.positional.get(index - 1) {
                                                    Some(v) => v.to_owned(),
                                                    None => {
                                                        panic!(
                                                            "no positional argument given for ${}",
                                                            index
                                                        );
                                                    }
                                                }
                                            } else {
                                                match environment.named.get(arg.trim_matches('$')) {
                                                    Some(v) => v.to_owned(),
                                                    None => {
                                                        panic!(
                                                    "no value specified for named argument: {}",
                                                    arg
                                                );
                                                    }
                                                }
                                            }
                                        } else {
                                            arg
                                        }
                                    })
                                    .collect(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect(),
            })
        }
    });

    // Once we have a list of items, we can now execute them.
    for item in items {
        match item {
            Item::Comment(comment) => {
                println!("{}", comment);
            }
            Item::Pipeline { cmds, literal } => {
                println!("{}", literal);

                let mut prev = None;
                let mut cmds = cmds.iter().peekable();

                while let Some(Cmd { name, args }) = cmds.next() {
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
                                    env::current_dir().expect("fetching current working directory"),
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

#[derive(Debug)]
struct Environment {
    named: HashMap<String, String>,
    positional: Vec<String>,
}

impl FromStr for Environment {
    type Err = Box<dyn Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut env = Environment {
            named: HashMap::new(),
            positional: Vec::new(),
        };

        let mut iter = s.split_whitespace().map(String::from);

        // Iterate over each argument.
        // If an argument appears like "-Flag value", create a named argument.
        // Else put the arg in positional vector.
        // Note: Named arguments must have a value.

        while let Some(arg) = iter.next() {
            if arg.starts_with("-") {
                let name = arg.trim_matches('-').to_owned();
                if let Some(value) = iter.next() {
                    if value.starts_with("-") {
                        return Err(format!("{} is missing a value", name).into());
                    }
                    env.named.insert(name, value);
                }
            } else {
                env.positional.push(arg);
            }
        }

        Ok(env)
    }
}

#[derive(Debug)]
struct Cmd {
    name: String,
    args: Vec<String>,
}

#[derive(Debug)]
enum Item {
    Comment(String),
    Pipeline { cmds: Vec<Cmd>, literal: String },
}

// rm the given glob pattern.
// Does what you expect: removes the files that match the pattern.
fn rm(pattern: &str) -> Result<(), Box<dyn Error>> {
    glob(pattern)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| std::fs::remove_file(&entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}
