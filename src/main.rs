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

    if let Some(mut run_file) = args.next() {
        if !run_file.ends_with(".run") {
            run_file.push_str(".run");
        }
        File::open(run_file)
            .expect("opening run file")
            .read_to_string(&mut file)
            .expect("reading run file");
    }

    // Wrap each unique argument in quotes for the environment parser.
    let s: String = args.fold(String::new(), |mut buf, next| {
        buf.push(' ');
        buf.push('"');
        buf.extend(next.chars());
        buf.push('"');
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

#[derive(Debug, PartialEq)]
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

        let mut iter = SplitWords { src: s.chars() };

        // Iterate over each argument.
        // If an argument appears like "-Flag value", create a named argument.
        // Else put the arg in positional vector.
        // Note: Named arguments must have a value.

        while let Some(arg) = iter.next() {
            if arg.starts_with("-") {
                let name = arg.trim_matches('-').to_owned();
                if let Some(value) = iter.next() {
                    dbg!(&value);
                    if value.starts_with("-") {
                        return Err(format!("{} is missing a value", arg).into());
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

struct ItemParser<'a> {
    env: &'a Environment,
}

// Parsing is done very simple, line-wise, semicolon-wise, then pipe-wise.
//
// Single command on a line:
//  command arg\n
//
// Multiple commands piped:
// Obviously, stdout of each command gets piped into stdin of the next.
//
//  command arg | command arg | command arg ; final_command\n
//  ^---------^   ^---------^   ^---------^   ^-----------^
//
// Only the actual command parsing requires the environment.
impl<'a> ItemParser<'a> {
    // Parse a string buffer into a list of command items.
    // Note: Reports the first error encountered and discards the rest.
    fn parse(&self, s: &str) -> Result<Vec<Item>, String> {
        Ok(s.lines()
            .map(|s| {
                if s.starts_with("//") {
                    Ok(vec![Item::Comment(s.into())])
                } else {
                    s.split(";")
                        .map(|s| s.split("|"))
                        .flatten()
                        .map(|s| self.parse_pipeline(s))
                        .collect()
                }
            })
            .collect::<Result<Vec<Vec<Item>>, String>>()?
            .into_iter()
            .flatten()
            .collect())
    }
    // Parse a pipeline of commands into a pipeline structure.
    fn parse_pipeline(&self, fragment: &str) -> Result<Item, String> {
        let mut cmds: Vec<Cmd> = vec![];
        let mut words = SplitWords {
            src: fragment.chars(),
        };

        if let Some(name) = words.next() {
            let cmd = Cmd {
                name: name.to_owned(),
                args: words
                    .map(String::from)
                    .map(|arg| {
                        // Basically, if arg is "$<numeric>" we parse
                        // the number and lookup the corresponding positional argument.
                        // If arg is "$<identifier>" we lookup the named argument.
                        // If either one doesn't exist we throw up an error.

                        // TODO(jfm): -Version 0.3.0 + "v$Version" -> "v0.3.0"

                        if arg.contains('$') {
                            if let Ok(index) = arg
                                .chars()
                                .skip(1)
                                .next()
                                .unwrap()
                                .to_string()
                                .parse::<usize>()
                            {
                                match self.env.positional.get(index - 1) {
                                    Some(v) => Ok(v.to_owned()),
                                    None => {
                                        Err(format!("no positional argument given for ${}", index))
                                    }
                                }
                            } else {
                                let mut parts = arg.split('$');
                                let prefix = parts.next().unwrap().trim();
                                let index = parts.next().unwrap().trim();
                                match self.env.named.get(index) {
                                    Some(v) => Ok(format!("{}{}", prefix, v)),
                                    None => Err(format!(
                                        "no value specified for named argument: {}",
                                        index,
                                    )),
                                }
                            }
                        } else {
                            Ok(arg)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            };
            cmds.push(cmd);
        }

        Ok(Item::Pipeline {
            cmds: cmds,
            literal: fragment.trim().into(),
        })
    }
}

#[derive(Debug, PartialEq)]
struct Cmd {
    name: String,
    args: Vec<String>,
}

#[derive(Debug, PartialEq)]
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

// SplitWords implements a custom definition of "word" that includes "delimited
// by whitespace, unless inside a string literal".
#[derive(Debug)]
struct SplitWords<Src>
where
    Src: Iterator<Item = char>,
{
    src: Src,
}

impl<Src> Iterator for SplitWords<Src>
where
    Src: Iterator<Item = char>,
{
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut word = String::new();
        while let Some(next) = self.src.next() {
            // Grab string literals as a single word, regardless of white
            // space.
            if next == '"' {
                while let Some(next) = self.src.next() {
                    if next == '"' {
                        return Some(word);
                    }
                    word.push(next);
                }
            }
            if next.is_whitespace() {
                // No point returning empty words.
                if word.is_empty() {
                    continue;
                } else {
                    return Some(word);
                }
            }
            word.push(next);
        }
        if word.is_empty() {
            None
        } else {
            Some(word)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! map(
        { $($key:expr => $value:expr),+ } => {
            {
                let mut m = ::std::collections::HashMap::new();
                $(
                    m.insert($key, $value);
                )+
                m
            }
         };
    );

    #[test]
    fn test_split_words() {
        let input = r#"-Message "feat: Run files without specifying the extension" -Version 0.3.0"#;
        let want = vec![
            "-Message",
            "feat: Run files without specifying the extension",
            "-Version",
            "0.3.0",
        ];
        let got: Vec<_> = SplitWords { src: input.chars() }.collect();
        assert_eq!(got, want);
    }

    #[test]
    fn test_environment_parsing() {
        let input = r#"-Message "feat: Run files without specifying the extension" -Version 0.3.0 foo bar baz"#;
        let positional = vec!["foo", "bar", "baz"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut named = HashMap::new();
        named.insert(
            "Message".to_owned(),
            "feat: Run files without specifying the extension".to_owned(),
        );
        named.insert("Version".to_owned(), "0.3.0".to_owned());
        let want = Environment { named, positional };
        let got = Environment::from_str(input).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn test_inline_variables() {
        let input = r#"ident v$Version ident"#;
        let want = vec![Item::Pipeline {
            literal: input.into(),
            cmds: vec![Cmd {
                name: "ident".into(),
                args: vec!["v0.3.0".into(), "ident".into()],
            }],
        }];
        let got = ItemParser {
            env: &Environment {
                named: map! {"Version".to_owned() => "0.3.0".to_owned()},
                positional: vec![],
            },
        }
        .parse(&input)
        .unwrap();
        assert_eq!(got, want);
    }
}
