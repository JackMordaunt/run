use crate::env::Environment;
use crate::util::SplitWords;
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Cmd {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Item {
    Comment(String),
    Pipeline { cmds: Vec<Cmd>, literal: String },
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
            .filter(|s| !s.trim().is_empty())
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
    // "cat src/main.rs | rg match | head 20"
    fn parse_pipeline(&self, fragment: &str) -> Result<Item, String> {
        let cmds = fragment
            .split("|")
            .map(|s| SplitWords { src: s.chars() })
            .map(|mut words| -> Result<Cmd, String> {
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
                                            None => Err(format!(
                                                "no positional argument given for ${}",
                                                index
                                            )),
                                        }
                                    } else {
                                        let mut parts = arg.split('$');
                                        let prefix = parts.next().unwrap();
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
                    Ok(cmd)
                } else {
                    Err("empty command".into())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Item::Pipeline {
            cmds,
            literal: fragment.trim().into(),
        })
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
                named: map! {"Version" => "0.3.0"},
                positional: vec![],
            },
        }
        .parse(&input)
        .unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn test_pipeline_parsing() {
        let input = r#"cat src/main.rs | rg match | head 5"#;
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
                cmds: vec![Cmd {
                    name: "one".into(),
                    args: vec![],
                }],
                literal: "one".into(),
            },
            Item::Pipeline {
                cmds: vec![Cmd {
                    name: "two".into(),
                    args: vec![],
                }],
                literal: "two".into(),
            },
            Item::Pipeline {
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
}
