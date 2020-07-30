use crate::parser::{Cmd, Item};
use glob::glob;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::process::{Child, Command, Stdio};

// Pipeline can arbitrarily execute, writing to `output` and reporting any
// errors it encounters.
pub trait Pipeline<Out>
where
    Out: Write,
{
    fn execute(&self, output: Out) -> Result<(), Box<dyn Error>>;
}

impl<Out> Pipeline<Out> for Item
where
    Out: Write,
{
    fn execute(&self, mut output: Out) -> Result<(), Box<dyn Error>> {
        if let Item::Pipeline { cmds, terminus, .. } = self {
            let mut prev = None;
            let mut cmds = cmds.into_iter().peekable();

            while let Some(cmd) = cmds.next() {
                write!(output, "{}\n", &cmd)?;
                let Cmd { name, args } = cmd;

                match name.as_ref() {
                    // Note(jfm):
                    //  Should builtins get access to pipes? Do they need them?
                    //  Should we check to see if an "rm" utility exists on the machine?
                    //  User would probably like to use their installed rm utitliy.
                    "rm" => {
                        args.iter()
                            .map(|arg| rm(arg))
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|e| format!("rm {}: {}", args.join(" "), e))?;
                    }
                    "cp" => {
                        let mut args = args.into_iter();
                        let (src, dst) = (args.next(), args.next());
                        match (src, dst) {
                            (Some(src), Some(dst)) => {
                                cp(src, dst).map_err(|e| format!("cp {} {}: {}", src, dst, e))?;
                            }
                            _ => {
                                return Err(
                                    format!("cp: invalid arguments: {:?} {:?}", src, dst).into()
                                );
                            }
                        };
                    }
                    _ => {
                        let stdin = prev.map_or(Stdio::inherit(), |output: Child| {
                            Stdio::from(output.stdout.unwrap())
                        });

                        let stdout = if cmds.peek().is_some() {
                            Stdio::piped()
                        } else {
                            if let Some(terminus) = &terminus {
                                File::create(terminus)
                                    .map_err(|e| format!("opening terminus file: {}", e))?
                                    .into()
                            } else {
                                Stdio::inherit()
                            }
                        };

                        let output = Command::new(&name)
                            .current_dir(std::env::current_dir().map_err(|e| {
                                format!("fetching current working directory: {}", e)
                            })?)
                            .args(args)
                            .stdin(stdin)
                            .stdout(stdout)
                            .spawn()
                            .map_err(|e| format!("{}: {}", &name, e))?;

                        prev = Some(output);
                    }
                };
            }

            if let Some(mut finish) = prev {
                finish.wait().ok();
            }
        }

        Ok(())
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

fn cp(src: &str, dst: &str) -> Result<(), Box<dyn Error>> {
    std::fs::copy(src, dst)?;
    Ok(())
}
