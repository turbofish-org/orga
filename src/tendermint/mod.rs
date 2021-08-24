use crate::error::Result;
use failure::bail;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug)]
struct ProcessHandler {
    command: std::process::Command,
    process: Option<std::process::Child>,
}

impl ProcessHandler {
    pub fn new(command: &str) -> Result<Self> {
        let command = Command::new(command);
        Ok(ProcessHandler {
            command,
            process: None,
        })
    }

    pub fn set_arg(&mut self, arg: &str) {
        self.command.arg(arg);
    }

    pub fn spawn(mut self) -> Result<()> {
        match self.process {
            Some(_) => bail!("Child process already spawned"),
            None => self.process = Some(self.command.spawn()?),
        };
        self.process.unwrap().wait().unwrap();
        Ok(())
    }

    pub fn kill(self) -> Result<()> {
        let mut child = match self.process {
            Some(inner) => inner,
            None => {
                bail!("Child process is not yet spawned. How do you kill that which has no life?");
            }
        };
        child.kill()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Tendermint {
    process: ProcessHandler,
    home: String,
}

impl Tendermint {
    pub fn new(home_path: &str) -> Result<Tendermint> {
        let path: PathBuf = home_path.into();
        if !path.exists() {
            fs::create_dir(path)?;
        }
        Ok(Tendermint {
            process: ProcessHandler::new("tendermint")?,
            home: home_path.to_string(),
        })
    }

    pub fn stdout<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stdout(cfg);
        self
    }

    pub fn stderr<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stderr(cfg);
        self
    }

    fn install() {
        unimplemented!();
    }

    pub fn home(mut self, new_home: &str) -> Self {
        self.process.set_arg("--home");
        self.process.set_arg(new_home);
        self
    }

    fn init(&self) {
        unimplemented!();
    }

    fn unsafe_reset_all(&self) {
        unimplemented!();
    }

    pub fn start(mut self) {
        self.process.set_arg("start");
        self.process.spawn().unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new() {
        let process = Tendermint::new(".tendermint").unwrap().home("testy_test");
        println!("{:?}", process);
    }
}
