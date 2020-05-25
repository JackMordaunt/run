use std::iter::Peekable;

// SplitWords implements a custom definition of "word" that includes "delimited
// by whitespace, unless inside a string literal".
//
// TODO: Arbitrarily quoted strings.
#[derive(Debug)]
pub(crate) struct SplitWords<Src>
where
    Src: Iterator<Item = char>,
{
    pub src: Peekable<Src>,
}

impl<Src> Iterator for SplitWords<Src>
where
    Src: Iterator<Item = char>,
{
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut word = String::new();

        while let Some(c) = self.src.next() {
            // Grab string literals as a single word, regardless of white
            // space.
            if c == '"' {
                while let (Some(next), peek) = (self.src.next(), self.src.peek()) {
                    match (next, peek) {
                        ('\\', Some('"')) => {
                            self.src.next();
                            word.push('\\');
                            word.push('"');
                        }
                        ('"', _) => {
                            return Some(word);
                        }
                        (next, _) => {
                            word.push(next);
                        }
                    };
                }
            }

            if c.is_whitespace() {
                // No point returning empty words.
                if word.is_empty() {
                    continue;
                } else {
                    return Some(word);
                }
            }

            word.push(c);
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
    use pretty_assertions::assert_eq;

    #[test]
    fn test_escaped_quotes() {
        let input = r#"foo bar "baz bazinga \"foobar\" " foobaz "#;
        let want = vec!["foo", "bar", "baz bazinga \\\"foobar\\\" ", "foobaz"];
        let got = SplitWords {
            src: input.chars().peekable(),
        }
        .collect::<Vec<_>>();
        assert_eq!(want, got);
    }
}
