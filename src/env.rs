use crate::util::SplitWords;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;

#[derive(Debug, Default, PartialEq)]
pub struct Environment {
    pub named: HashMap<String, String>,
    pub positional: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
