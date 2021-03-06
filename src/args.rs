//! nix-shell argument parsing
//!
//! While `cached-nix-shell` passes most of its arguments to `nix-shell` as-is
//! without even looking into them, there are some arguments that should be
//! extracted and processed by `cached-nix-shell` itself.  In order to do so, we
//! still need to parse the whole command line.
//!
//! **Q:** Why not just use a library like `clap` or `docopts`?
//! **A:** We need to emulate quirks of nix-shell argument parsing in a 100%
//! compatible way, so it is appropriate to code this explicitly rather than use
//! such libraries.

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use ufcs::Pipe;

pub enum RunMode {
    /// no arg
    InteractiveShell,
    /// --run CMD | --command CMD
    Shell(OsString),
    /// --exec CMD ARGS...
    Exec(OsString, Vec<OsString>),
}

pub struct Args {
    /// true: -p | --packages
    pub packages: bool,
    /// true: --pure; false: --impure
    pub pure: bool,
    /// -i (in shebang)
    pub interpreter: OsString,
    /// --run | --command | --exec (not in shebang)
    pub run: RunMode,
    /// other positional arguments (after --)
    pub rest: Vec<OsString>,
    /// other keyword arguments
    pub other_kw: Vec<OsString>,
}

impl Args {
    pub fn parse(
        args: Vec<OsString>,
        in_shebang: bool,
    ) -> Result<Args, String> {
        let mut res = Args {
            packages: false,
            pure: false,
            interpreter: OsString::from("bash"),
            run: RunMode::InteractiveShell,
            rest: Vec::new(),
            other_kw: Vec::new(),
        };
        let mut it = VecDeque::<OsString>::from(args);
        while let Some(arg) = get_next_arg(&mut it) {
            let mut next = || -> Result<OsString, String> {
                it.pop_front()
                    .ok_or_else(|| {
                        format!("flag {:?} requires more arguments", arg)
                    })?
                    .clone()
                    .pipe(Ok)
            };
            if arg == "--attr" || arg == "-A" {
                res.other_kw.extend(vec!["-A".into(), next()?]);
            } else if arg == "-I" {
                res.other_kw.extend(vec!["-I".into(), next()?]);
            } else if arg == "--arg" {
                res.other_kw.extend(vec!["--arg".into(), next()?, next()?]);
            } else if arg == "--argstr" {
                res.other_kw
                    .extend(vec!["--argstr".into(), next()?, next()?]);
            } else if arg == "--option" {
                res.other_kw
                    .extend(vec!["--option".into(), next()?, next()?]);
            } else if arg == "-j" || arg == "--max-jobs" {
                res.other_kw.extend(vec!["--max-jobs".into(), next()?]);
            } else if arg == "--pure" {
                res.pure = true;
            } else if arg == "--impure" {
                res.pure = false;
            } else if arg == "--packages" || arg == "-p" {
                res.packages = true;
            } else if arg == "-i" && in_shebang {
                res.interpreter = next()?;
            } else if (arg == "--run" || arg == "--command") && !in_shebang {
                res.run = RunMode::Shell(next()?);
            } else if arg == "--exec" && !in_shebang {
                res.run = RunMode::Exec(next()?, it.into());
                break;
            } else if arg.as_bytes().first() == Some(&b'-') {
                return Err(format!("unexpected arg {:?}", arg));
            } else {
                res.rest.push(arg.clone());
            }
        }
        Ok(res)
    }
}

fn get_next_arg(it: &mut VecDeque<OsString>) -> Option<OsString> {
    let arg = it.pop_front()?;
    let argb = arg.as_bytes();
    if argb.len() > 2 && argb[0] == b'-' && is_alpha(argb[1]) {
        // Expand short options and put them back to the deque.
        // Reference: https://github.com/NixOS/nix/blob/2.3.1/src/libutil/args.cc#L29-L42

        let split_idx = argb[1..]
            .iter()
            .position(|&b| !is_alpha(b))
            .unwrap_or(argb.len() - 1);
        // E.g. "-pj16" -> ("pj", "16")
        let (letters, rest) = argb[1..].split_at(split_idx);

        if rest.len() != 0 {
            it.push_front(OsStr::from_bytes(rest).into());
        }
        for &c in letters.iter().rev() {
            it.push_front(OsStr::from_bytes(&[b'-', c]).into());
        }

        it.pop_front()
    } else {
        Some(arg)
    }
}

fn is_alpha(b: u8) -> bool {
    b'a' <= b && b <= b'z' || b'A' <= b && b <= b'Z'
}

#[cfg(test)]
mod test {
    use super::*;
    /// Expand an arg using `get_next_arg`
    fn expand(arg: &str) -> Vec<String> {
        let mut it: VecDeque<OsString> = VecDeque::from(vec![arg.into()]);
        std::iter::from_fn(|| get_next_arg(&mut it))
            .map(|s| s.to_string_lossy().into())
            .collect()
    }
    #[test]
    fn test_get_next_arg() {
        assert_eq!(expand("--"), vec!["--"]);
        assert_eq!(expand("default.nix"), vec!["default.nix"]);
        assert_eq!(expand("--argstr"), vec!["--argstr"]);
        assert_eq!(expand("-pi"), vec!["-p", "-i"]);
        assert_eq!(expand("-j4"), vec!["-j", "4"]);
        assert_eq!(expand("-j16"), vec!["-j", "16"]);
        assert_eq!(expand("-pj16"), vec!["-p", "-j", "16"]);
    }
}
