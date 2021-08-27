use crate::error::Result;
use datetime::LocalTime;
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
use toml_edit::{value, Document};

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
            Some(process) => process.wait()?,
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
    config_contents: Option<toml_edit::Document>,
}

impl Tendermint {
    /// Constructs a new `Tendermint` for handling tendermint processes
    /// from Rust programs
    ///
    /// Passed home_path generates and enclosing directory which will, in
    /// addition to housing the downloaded tendermint binary, will serve
    /// as the tendermint --home argument
    pub fn new<T: Into<PathBuf> + Clone>(home_path: T) -> Tendermint {
        let path: PathBuf = home_path.clone().into();
        if !path.exists() {
            fs::create_dir(path.clone()).expect("Failed to create Tendermint home directory");
        }
        let tendermint = Tendermint {
            process: ProcessHandler::new("tendermint"),
            home: home_path.clone().into(),
            genesis_path: None,
            config_contents: None,
        };
        tendermint.home(home_path.into())
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

    fn home(mut self, home_path: PathBuf) -> Self {
        let new_home = home_path.to_str().unwrap();
        self.process.set_arg("--home");
        self.process.set_arg(new_home);
        self
    }

    /// Log Level
    /// Sets the --log_level argument (default `info`)
    ///
    /// Compatible Commands:
    ///     start
    ///     init
    ///     unsafe_reset_all
    pub fn log_level(mut self, level: &str) -> Self {
        self.process.set_arg("--log_level");
        self.process.set_arg(level);
        self
    }

    /// Prints out full stack trace on errors
    /// Sets the --trace argument
    ///
    /// Compatible Commands:
    ///     start
    ///     init
    ///     unsafe_reset_all
    pub fn trace(mut self) -> Self {
        self.process.set_arg("--trace");
        self
    }

    /// Node name
    /// Sets the --moniker argument
    ///
    /// Compatible Commands:
    ///     start
    pub fn moniker(mut self, moniker: &str) -> Self {
        self.process.set_arg("--moniker");
        self.process.set_arg(moniker);
        self
    }

    /// Node listen address
    /// Default: "tcp://0.0.0.0:26656"
    /// 0.0.0.0:0 means any interface, any port
    /// Sets the --p2p.ladder argument
    ///
    /// Compatible Commands:
    ///     start
    ///
    /// Note: Using this configuration command with incompatible
    /// terminating methods will cause the tendermint process to fail
    pub fn p2p_laddr(mut self, addr: &str) -> Self {
        self.process.set_arg("--p2p.laddr");
        self.process.set_arg(addr);
        self
    }

    /// Persistent peers
    /// Format: ID@host:port
    /// Sets the --p2p.persistent_peers argument
    ///
    /// Compatible Commands:
    ///     start
    ///
    /// Note: Using this configuration command with incompatible
    /// terminating methods will cause the tendermint process to fail
    pub fn p2p_persistent_peers<const N: usize>(mut self, peers: [&str; N]) -> Self {
        self.process.set_arg("--p2p.persistent_peers");
        let mut arg: String = "".to_string();
        peers.iter().for_each(|x| arg += &x.to_string());
        self.process.set_arg(&arg);
        self
    }

    /// RPC listen address
    /// Port required
    /// Default "tcp://127.0.0.1:26657"
    /// Sets the --rpc.laddr argument
    ///
    /// Compatible Commands:
    ///     start
    ///
    /// Note: Using this configuration command with incompatible
    /// terminating methods will cause the tendermint process to fail
    pub fn rpc_laddr(mut self, addr: &str) -> Self {
        self.process.set_arg("--rpc.laddr");
        self.process.set_arg(addr);
        self
    }

    /// Stdout target
    ///
    /// # Examples
    ///
    /// Discard output:
    ///
    /// ```no_run
    /// use orga::tendermint::Tendermint;
    /// use std::process::Stdio;
    ///
    /// Tendermint::new("tendermint")
    ///     .stdout(Stdio::null())
    ///     .start();
    /// ```
    ///
    /// Pipe output to file:
    ///
    /// ```no_run
    /// use orga::tendermint::Tendermint;
    /// use std::fs::File;
    ///
    /// let log_file = File::create("log.txt").unwrap();
    ///
    /// Tendermint::new("tendermint")
    ///     .stdout(log_file)
    ///     .start();
    /// ```
    pub fn stdout<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stdout(cfg);
        self
    }

    /// Stderr target
    ///
    /// # Examples
    ///
    /// Discard output:
    ///
    /// ```no_run
    /// use orga::tendermint::Tendermint;
    /// use std::process::Stdio;
    ///
    /// Tendermint::new("tendermint")
    ///     .stderr(Stdio::null())
    ///     .start();
    /// ```
    ///
    /// Pipe output to file:
    ///
    /// ```no_run
    /// use orga::tendermint::Tendermint;
    /// use std::fs::File;
    ///
    /// let log_file = File::create("log.txt").unwrap();
    ///
    /// Tendermint::new("tendermint")
    ///     .stderr(log_file)
    ///     .start();
    /// ```
    pub fn stderr<T: Into<Stdio>>(mut self, cfg: T) -> Self {
        self.process.command.stderr(cfg);
        self
    }

    /// Keep the address book intact
    ///
    /// Compatible Commands:
    ///     unsafe_reset_all
    ///
    /// Note: Using this configuration command with incompatible
    /// terminating methods will cause the tendermint process to fail
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

    /// Copies the contents of the file at passed path to the genesis.json
    /// file located in the config directory of the tendermint home
    ///
    /// Compatible Commands:
    ///     start
    ///     init
    ///
    /// Note: This copy happens upon calling a terminating method in order to
    /// ensure file copy is not overwritten by called tendermint process
    pub fn with_genesis(mut self, path: PathBuf) -> Self {
        self.genesis_path = Some(path);
        self
    }

    fn read_config_toml(&mut self) {
        let config_path = self.home.join("config/config.toml");
        let contents = fs::read_to_string(config_path.clone()).unwrap();
        let document = contents
            .parse::<Document>()
            .expect("Invalid config.toml contents");
        self.config_contents = Some(document);
    }

    fn write_config_toml(&self) {
        let document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                return;
            }
        };
        let config_path = self.home.join("config/config.toml");
        fs::write(config_path, document.to_string())
            .expect("Unable to write modified config.toml to file.");
    }

    fn mutate_configuration(&self) {
        self.apply_genesis();
        self.write_config_toml();
    }

    /// Edits the statesync enable located in the config.toml in the
    /// config directory under the tendermint home
    ///
    /// Fully enabling state sync requires the configuration of rpc_servers,
    /// trust height, and trust hash
    ///
    /// Note: This update happens upon calling a terminating method in order to
    /// ensure a single file read and to ensure that the config.toml is not
    /// overwritten by called tendermint process
    pub fn state_sync(mut self, enable: bool) -> Self {
        let mut document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                self.read_config_toml();
                self.config_contents.unwrap()
            }
        };

        document["statesync"]["enable"] = value(enable);

        self.config_contents = Some(document);
        self
    }

    /// Edits the statesync rpc_servers located in the config.toml in the
    /// config directory under the tendermint home
    ///
    /// Two rpc servers are required to enable state sync
    ///
    /// Note: This update happens upon calling a terminating method in order to
    /// ensure a single file read and to ensure that the config.toml is not
    /// overwritten by called tendermint process
    pub fn rpc_servers<const N: usize>(mut self, rpc_servers: [&str; N]) -> Self {
        let mut document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                self.read_config_toml();
                self.config_contents.unwrap()
            }
        };

        let mut rpc_string: String = "".to_string();
        rpc_servers.iter().for_each(|item| {
            rpc_string += item;
            rpc_string += ",";
        });

        document["statesync"]["rpc_servers"] = value(rpc_string);

        self.config_contents = Some(document);
        self
    }

    /// Edits the statesync trust_height located in the config.toml in the
    /// config directory under the tendermint home
    ///
    /// Note: This update happens upon calling a terminating method in order to
    /// ensure a single file read and to ensure that the config.toml is not
    /// overwritten by called tendermint process
    pub fn trust_height(mut self, height: u32) -> Self {
        let mut document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                self.read_config_toml();
                self.config_contents.unwrap()
            }
        };

        document["statesync"]["trust_height"] = value(height as i64);

        self.config_contents = Some(document);
        self
    }

    /// Edits the statesync trust_hash located in the config.toml in the
    /// config directory under the tendermint home
    ///
    /// Note: This update happens upon calling a terminating method in order to
    /// ensure a single file read and to ensure that the config.toml is not
    /// overwritten by called tendermint process
    pub fn trust_hash(mut self, hash: &str) -> Self {
        let mut document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                self.read_config_toml();
                self.config_contents.unwrap()
            }
        };

        document["statesync"]["trust_hash"] = value(hash);

        self.config_contents = Some(document);
        self
    }

    /// Edits the block time located in the config.toml in the config directory under the
    /// tendermint home
    ///
    /// Compatible Commands:
    ///     start
    ///     init
    ///
    /// Note: This update happens upon calling a terminating method in order to
    /// ensure a single file read and to ensure that the config.toml is not
    /// overwritten by called tendermint process
    pub fn block_time(mut self, time: &str) -> Self {
        let mut document = match &self.config_contents {
            Some(inner) => inner.clone(),
            None => {
                self.read_config_toml();
                self.config_contents.unwrap()
            }
        };

        document["consensus"]["timeout_commit"] = value(time);

        self.config_contents = Some(document);
        self
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub fn start(mut self) {
        self.install();
        self.mutate_configuration();
        self.process.set_arg("start");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
    }

    /// Calls tendermint init with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub fn init(mut self) {
        self.install();
        self.process.set_arg("init");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
        self.mutate_configuration();
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub fn unsafe_reset_all(mut self) {
        self.install();
        self.process.set_arg("unsafe_reset_all");
        self.process.spawn().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempdir::TempDir;

    #[test]
    #[ignore]
    fn tendermint_init() {
        let temp_dir = TempDir::new("tendermint_test").unwrap();
        let temp_dir_path = temp_dir.path();
        Tendermint::new(temp_dir_path).stdout(Stdio::null()).init();

        let file_set: HashSet<String> = temp_dir_path
            .read_dir()
            .unwrap()
            .map(|x| x.unwrap().file_name().to_str().unwrap().to_string())
            .collect();

        let expected: HashSet<String> = HashSet::from([
            "config".to_string(),
            "data".to_string(),
            "tendermint-v0.34.11".to_string(),
        ]);

        assert_eq!(file_set, expected);
    }
}
