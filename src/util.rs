// SplitWords implements a custom definition of "word" that includes "delimited
// by whitespace, unless inside a string literal".
#[derive(Debug)]
pub(crate) struct SplitWords<Src>
where
    Src: Iterator<Item = char>,
{
    pub src: Src,
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
