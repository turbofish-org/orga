use crate::error::Result;
use failure::bail;
use flate2::read::GzDecoder;
use hex_literal::hex;
use is_executable::IsExecutable;
use log::info;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tar::Archive;

#[cfg(target_os = "macos")]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.11/tendermint_0.34.11_darwin_amd64.tar.gz";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.11/tendermint_0.34.11_linux_amd64.zip";
#[cfg(all(target_os = "linux", target_arch = "arm"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.11/tendermint_0.34.11_linux_arm64.zip";

#[cfg(target_os = "macos")]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("e565ec1b90a950093d7d77745f1579d87322f5900c67ec51ff2cd02b988b6d52");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("1496d3808ed1caaf93722983f34d4ba38392ea6530f070a09e7abf1ea4cc5106");
#[cfg(all(target_os = "linux", target_arch = "arm"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("01a076d3297a5381587a77621b7f45dca7acb7fc21ce2e29ca327ccdaee41757");

fn verify_hash(tendermint_bytes: &[u8]) {
    let mut hasher = Sha256::new();
    hasher.update(tendermint_bytes);
    let digest = hasher.finalize();
    let bytes = digest.as_slice();
    assert_eq!(
        bytes, TENDERMINT_ZIP_HASH,
        "Tendermint binary zip did not match expected hash"
    );
    info!("Confirmed correct Tendermint zip hash");
}

#[derive(Debug)]
struct ProcessHandler {
    command: std::process::Command,
    process: Option<std::process::Child>,
}

impl ProcessHandler {
    pub fn new(command: &str) -> Self {
        let command = Command::new(command);
        ProcessHandler {
            command,
            process: None,
        }
    }

    pub fn set_arg(&mut self, arg: &str) {
        self.command.arg(arg);
    }

    pub fn spawn(&mut self) -> Result<()> {
        match self.process {
            Some(_) => bail!("Child process already spawned"),
            None => self.process = Some(self.command.spawn()?),
        };
        Ok(())
    }

    pub fn wait(&mut self) -> Result<()> {
        match &mut self.process {
            Some(process) => process.wait().unwrap(),
            None => bail!("Child process not yet spawned."),
        };
        Ok(())
    }

    #[allow(dead_code)]
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
    home: PathBuf,
    genesis_path: Option<PathBuf>,
}

impl Tendermint {
    pub fn new(home_path: &str) -> Tendermint {
        let path: PathBuf = home_path.into();
        if !path.exists() {
            fs::create_dir(path.clone()).expect("Failed to create Tendermint home directory");
        }
        let tendermint = Tendermint {
            process: ProcessHandler::new("tendermint"),
            home: home_path.into(),
            genesis_path: None,
        };
        tendermint.home(home_path)
    }

    fn install(&self) {
        let tendermint_path = self.home.join("tendermint-v0.34.11");

        if tendermint_path.is_executable() {
            info!("Tendermint already installed");
            return;
        }

        info!("Installing Tendermint to {}", self.home.to_str().unwrap());
        let mut buf: Vec<u8> = vec![];
        reqwest::blocking::get(TENDERMINT_BINARY_URL)
            .expect("Failed to download Tendermint zip file from GitHub")
            .copy_to(&mut buf)
            .expect("Failed to read bytes from zip file");

        info!("Downloaded Tendermint binary");
        verify_hash(&buf);

        let cursor = std::io::Cursor::new(buf.clone());
        let tar = GzDecoder::new(cursor);
        let mut archive = Archive::new(tar);

        for item in archive.entries().unwrap() {
            if item.as_ref().unwrap().path().unwrap().to_str().unwrap() == "tendermint" {
                let tendermint_bytes: Vec<u8> =
                    item.unwrap().bytes().map(|byte| byte.unwrap()).collect();

                let mut f = fs::File::create(tendermint_path)
                    .expect("Could not create Tendermint binary on file system");
                f.write_all(tendermint_bytes.as_slice())
                    .expect("Failed to write Tendermint binary to file system");

                break;
            }
        }
    }

    fn home(mut self, new_home: &str) -> Self {
        self.process.set_arg("--home");
        self.process.set_arg(new_home);
        self
    }

    pub fn log_level(mut self, level: &str) -> Self {
        self.process.set_arg("--log_level");
        self.process.set_arg(level);
        self
    }

    pub fn trace(mut self) -> Self {
        self.process.set_arg("--trace");
        self
    }

    pub fn moniker(mut self, moniker: &str) -> Self {
        self.process.set_arg("--moniker");
        self.process.set_arg(moniker);
        self
    }

    pub fn p2p_laddr(mut self, addr: &str) -> Self {
        self.process.set_arg("--p2p.laddr");
        self.process.set_arg(addr);
        self
    }

    //this should maybe take a list and be a const generic over any number of items?
    //but would be annoying with parsing that list and generating a new string that concatenates
    //them all
    pub fn p2p_persistent_peers(mut self, peers: &str) -> Self {
        self.process.set_arg("--p2p.persistent_peers");
        self.process.set_arg(peers);
        self
    }

    pub fn rpc_laddr(mut self, addr: &str) -> Self {
        self.process.set_arg("--rpc.laddr");
        self.process.set_arg(addr);
        self
    }

    pub fn stdout<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stdout(cfg);
        self
    }

    pub fn stderr<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stderr(cfg);
        self
    }

    //only for unsafe reset all
    pub fn keep_addr_book(mut self) -> Self {
        self.process.set_arg("--keep_addr_book");
        self
    }

    fn apply_genesis(&self) {
        let path = match &self.genesis_path {
            Some(inner) => inner,
            None => {
                return;
            }
        };
        let file_name = path.file_name().unwrap().clone();
        if file_name != "genesis.json" {
            //TODO: more sophisticated method to ensure that the file is a valid genesis.json
            panic!("Provided file is not a genesis.json.");
        }

        Command::new("cp")
            .arg(path.clone())
            .arg(self.home.join("config").join(file_name))
            .spawn()
            .expect("Failed to spawn genesis.json copy process.")
            .wait()
            .expect("genesis.json copy process failed.");
    }

    pub fn with_genesis(mut self, path: PathBuf) -> Self {
        self.genesis_path = Some(path);
        self
    }

    //any top level command will need to first try to install before anything happens
    //just need to make sure that this check happens before anything else in the install logic
    //and that it isn't too heavy
    pub fn start(mut self) {
        self.install();
        self.apply_genesis();
        self.process.set_arg("start");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
    }

    pub fn init(mut self) {
        self.install();
        self.process.set_arg("init");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
        self.apply_genesis();
    }

    pub fn unsafe_reset_all(mut self) {
        self.install();
        self.process.set_arg("unsafe_reset_all");
        self.process.spawn().unwrap();
    }
}
