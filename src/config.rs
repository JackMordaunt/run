use std::iter::Peekable;

#[derive(Default, Debug)]
pub struct Config {
    pub dry_run: bool,
}

impl Config {
    /// Consumes a stream of strings and parses flags into config values.
    /// Only actually consumes the values recognised by Config.
    /// Returns on the first unrecognised value.
    pub fn from_args<Args, Str>(args: &mut Peekable<Args>) -> Self
    where
        Args: Iterator<Item = Str>,
        Str: AsRef<str>,
    {
        let mut config = Config::default();

        while let Some(arg) = args.peek() {
            match arg.as_ref() {
                "--dry-run" | "--dry" => {
                    config.dry_run = true;
                }
                _ => {
                    break;
                }
            }
            args.next();
        }
        config
    }
}
