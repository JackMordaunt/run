use crate::env::Environment;
use crate::util::SplitWords;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Cmd {
    pub name: String, // Should this actually be a PathBuf?
    pub args: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Item {
    Comment(String),
    Pipeline {
        cmds: Vec<Cmd>,
        // Terminus is the final destination for a pipeline.
        // Specifies to stream output into the file.
        terminus: Option<PathBuf>,
        ignore_failure: bool,
        literal: String,
    },
}

pub struct ItemParser<'a> {
    pub env: &'a Environment,
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
    pub fn parse(&self, s: &str) -> Result<Vec<Item>, String> {
        Ok(s.lines()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with("//") {
                    Ok(vec![Item::Comment(s.into())])
                } else {
                    s.split(";").map(|s| self.parse_pipeline(s)).collect()
                }
            })
            .collect::<Result<Vec<Vec<Item>>, String>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    // Parse a pipeline of commands into a pipeline structure.
    // "cat src/main.rs | rg match | head > output.txt"
    fn parse_pipeline(&self, s: &str) -> Result<Item, String> {
        let literal = s;

        let (s, ignore_failure) = if s.starts_with("- ") {
            (s.trim_start_matches("- "), true)
        } else {
            (s, false)
        };

        let mut terminus = None;
        let mut cmds = s.split(" | ").collect::<Vec<_>>();

        let last = match cmds.last() {
            Some(last) => last,
            None => return Err("no commands to parse".into()),
        };

        // If the final command contains a " > ", break it off and use it as the
        // terminus redirection.
        // Note: Doesn't consider bad input like " > > ".
        if let Some(index) = last.find(" > ") {
            let (last, t) = last.split_at(index);
            cmds.remove(cmds.len() - 1);
            cmds.push(last);
            terminus = Some(t.trim_start_matches(" > ").into());
        }

        let cmds = cmds
            .into_iter()
            .map(|s| SplitWords {
                src: s.chars().peekable(),
            })
            .map(|mut words| -> Result<Cmd, String> {
                match words.next() {
                    Some(name) => Ok(Cmd {
                        name: name.to_owned(),
                        args: words
                            .map(String::from)
                            .map(|arg| self.parse_argument(arg))
                            .collect::<Result<Vec<_>, _>>()?,
                    }),
                    None => Err("empty command".into()),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Item::Pipeline {
            cmds,
            terminus,
            ignore_failure,
            literal: literal.into(),
        })
    }

    fn parse_argument(&self, arg: String) -> Result<String, String> {
        // Basically, if arg is "$(<numeric>)" we parse
        // the number and lookup the corresponding positional argument.
        // If arg is "$(<identifier>)" we lookup the named argument.
        // If either one doesn't exist we throw up an error.
        if arg.contains('$') {
            let mut ident = String::new();
            let mut prefix = String::new();
            let mut suffix = String::new();
            let mut stream = arg.chars().peekable();

            while let Some(c) = stream.next() {
                if c == '$' {
                    if let Some(p) = stream.peek() {
                        if *p == '(' {
                            stream.next();
                            while let Some(c) = stream.next() {
                                if c == ')' {
                                    break;
                                }
                                ident.push(c);
                            }
                            while let Some(c) = stream.next() {
                                suffix.push(c);
                            }
                        }
                    } else {
                        prefix.push(c);
                    }
                } else {
                    prefix.push(c);
                }
            }

            let value = match ident.parse::<usize>() {
                Ok(index) => self.env.positional.get(index - 1),
                Err(_) => self.env.named.get(&ident),
            };

            match value {
                Some(value) => Ok(format!("{}{}{}", prefix, value, suffix)),
                None => Err(format!("no value specified for argument: {}", ident,)),
            }
        } else {
            Ok(arg)
        }
    }
}

impl fmt::Display for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.name, self.args.join(" "))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    macro_rules! map(
        { $($key:expr => $value:expr),+ } => {
            {
                let mut m = ::std::collections::HashMap::new();
                $(
                    m.insert($key.into(), $value.into());
                )+
                m
            }
         };
    );

    #[test]
    fn test_inline_variables() {
        let input = r#"ident v$(Version) $(Bin).exe"#;
        let want = vec![Item::Pipeline {
            ignore_failure: false,
            terminus: None,
            literal: input.into(),
            cmds: vec![Cmd {
                name: "ident".into(),
                args: vec!["v0.3.0".into(), "binary.exe".into()],
            }],
        }];
        let got = ItemParser {
            env: &Environment {
                named: map! {"Version" => "0.3.0", "Bin" => "binary"},
                positional: vec![],
            },
        }
        .parse(&input)
        .unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn test_positional_variable() {
        let input = r#"ident v$(1) $(2).exe"#;
        let want = vec![Item::Pipeline {
            ignore_failure: false,
            terminus: None,
            literal: input.into(),
            cmds: vec![Cmd {
                name: "ident".into(),
                args: vec!["v0.3.0".into(), "binary.exe".into()],
            }],
        }];
        let got = ItemParser {
            env: &Environment {
                named: HashMap::new(),
                positional: vec!["0.3.0".into(), "binary".into()],
            },
        }
        .parse(&input)
        .unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn test_pipeline_parsing() {
        let input = r#"cat src/main.rs | rg "|" | head 5"#;
        let want = vec![
            Cmd {
                name: "cat".into(),
                args: vec!["src/main.rs".into()],
            },
            Cmd {
                name: "rg".into(),
                args: vec!["|".into()],
            },
            Cmd {
                name: "head".into(),
                args: vec!["5".into()],
            },
        ];
        let got = ItemParser {
            env: &Environment::default(),
        }
        .parse(&input)
        .expect("parsing");
        assert_eq!(
            got,
            vec![Item::Pipeline {
                ignore_failure: false,
                terminus: None,
                cmds: want,
                literal: input.into()
            }]
        );
    }

    #[test]
    fn test_file_redirection() {
        let input = r#"cat src/main.rs | rg match | head 5 > output.txt"#;
        let want = vec![
            Cmd {
                name: "cat".into(),
                args: vec!["src/main.rs".into()],
            },
            Cmd {
                name: "rg".into(),
                args: vec!["match".into()],
            },
            Cmd {
                name: "head".into(),
                args: vec!["5".into()],
            },
        ];
        let got = ItemParser {
            env: &Environment::default(),
        }
        .parse(&input)
        .expect("parsing");
        assert_eq!(
            got,
            vec![Item::Pipeline {
                ignore_failure: false,
                terminus: Some("output.txt".into()),
                cmds: want,
                literal: input.into()
            }]
        );
    }

    #[test]
    fn test_skip_empty_lines() {
        let input = r#"

        one


        two

        three
        
        "#;
        let want = vec![
            Item::Pipeline {
                ignore_failure: false,
                terminus: None,
                cmds: vec![Cmd {
                    name: "one".into(),
                    args: vec![],
                }],
                literal: "one".into(),
            },
            Item::Pipeline {
                ignore_failure: false,
                terminus: None,
                cmds: vec![Cmd {
                    name: "two".into(),
                    args: vec![],
                }],
                literal: "two".into(),
            },
            Item::Pipeline {
                ignore_failure: false,
                terminus: None,
                cmds: vec![Cmd {
                    name: "three".into(),
                    args: vec![],
                }],
                literal: "three".into(),
            },
        ];
        let got = ItemParser {
            env: &Environment::default(),
        }
        .parse(&input)
        .expect("parsing");
        assert_eq!(got, want);
    }

    #[test]
    fn test_ignore_failure() {
        let input = r#"- cat src/main.rs | rg match | head 5 > output.txt"#;
        let want = vec![
            Cmd {
                name: "cat".into(),
                args: vec!["src/main.rs".into()],
            },
            Cmd {
                name: "rg".into(),
                args: vec!["match".into()],
            },
            Cmd {
                name: "head".into(),
                args: vec!["5".into()],
            },
        ];
        let got = ItemParser {
            env: &Environment::default(),
        }
        .parse(&input)
        .expect("parsing");
        assert_eq!(
            got,
            vec![Item::Pipeline {
                ignore_failure: true,
                terminus: Some("output.txt".into()),
                cmds: want,
                literal: input.into()
            }]
        );
    }
}
