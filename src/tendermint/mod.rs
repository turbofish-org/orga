use crate::error::{Error, Result};
use flate2::read::GzDecoder;
use hex_literal::hex;
use is_executable::IsExecutable;
use log::{debug, info, trace};
use nom::bytes::complete::take_until;
use nom::character::complete::alphanumeric1;
use nom::multi::{many0, many1};
use nom::sequence::separated_pair;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{prelude::*, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use tar::Archive;
use toml_edit::{value, Document};

#[cfg(target_os = "macos")]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.15/tendermint_0.34.15_darwin_amd64.tar.gz";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.15/tendermint_0.34.15_linux_amd64.tar.gz";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/tendermint/tendermint/releases/download/v0.34.15/tendermint_0.34.15_linux_arm64.tar.gz";

#[cfg(target_os = "macos")]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("b493354bc8a711b670763e3ddf5765c3d7e94aaf6dbd138b16b8ab288495a4d1");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("cf4bd4b5a57f49007d18b9287214daf364dbc11094dec8e4c1bc33f207c6c57c");
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("6d4d771ae26c207f1a4f9f1399db2cbcac2e3c8afdf5d55d15bb984bbb986d2e");

const TENDERMINT_BINARY_NAME: &str = "tendermint-v0.34.15";

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
            Some(_) => {
                return Err(Error::Tendermint("Child process already spawned".into()));
            }
            None => self.process = Some(self.command.spawn()?),
        };
        Ok(())
    }

    pub fn wait(&mut self) -> Result<()> {
        match &mut self.process {
            Some(process) => process.wait()?,
            None => {
                return Err(Error::Tendermint("Child process not yet spawned".into()));
            }
        };
        Ok(())
    }

    #[allow(dead_code)]
    pub fn kill(self) -> Result<()> {
        let mut child = match self.process {
            Some(inner) => inner,
            None => {
                return Err(Error::Tendermint(
                    "Child process is not yet spawned. How do you kill that which has no life?"
                        .into(),
                ));
            }
        };
        child.kill()?;
        child.wait()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Tendermint {
    process: ProcessHandler,
    home: PathBuf,
    genesis_bytes: Option<Vec<u8>>,
    config_contents: Option<toml_edit::Document>,
    show_logs: bool,
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
        let tm_bin_path = path.join(TENDERMINT_BINARY_NAME);
        let tendermint = Tendermint {
            process: ProcessHandler::new(tm_bin_path.to_str().unwrap()),
            home: home_path.clone().into(),
            genesis_bytes: None,
            config_contents: None,
            show_logs: false,
        };
        tendermint.home(home_path.into())
    }

    fn install(&self) {
        let tendermint_path = self.home.join(TENDERMINT_BINARY_NAME);

        if tendermint_path.is_executable() {
            debug!("Tendermint already installed");
            return;
        }

        info!("Installing Tendermint to {}", self.home.to_str().unwrap());
        let mut buf: Vec<u8> = vec![];
        reqwest::blocking::get(TENDERMINT_BINARY_URL)
            .expect("Failed to download Tendermint zip file from GitHub")
            .copy_to(&mut buf)
            .expect("Failed to read bytes from zip file");

        verify_hash(&buf);

        let cursor = std::io::Cursor::new(buf.clone());
        let tar = GzDecoder::new(cursor);
        let mut archive = Archive::new(tar);

        for item in archive.entries().unwrap() {
            if item.as_ref().unwrap().path().unwrap().to_str().unwrap() == "tendermint" {
                let mut tendermint_bytes = vec![];
                item.unwrap().read_to_end(&mut tendermint_bytes).unwrap();

                let mut f = fs::File::create(tendermint_path)
                    .expect("Could not create Tendermint binary on file system");
                f.write_all(tendermint_bytes.as_slice())
                    .expect("Failed to write Tendermint binary to file system");

                let mut perms = f.metadata().unwrap().permissions();
                perms.set_mode(0o755);
                f.set_permissions(perms)
                    .expect("Failed to set Tendermint binary permissions");
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
    #[must_use]
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
    #[must_use]
    pub fn trace(mut self) -> Self {
        self.process.set_arg("--trace");
        self
    }

    /// Node name
    /// Sets the --moniker argument
    ///
    /// Compatible Commands:
    ///     start
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn p2p_persistent_peers(mut self, peers: Vec<String>) -> Self {
        self.process.set_arg("--p2p.persistent_peers");
        let mut arg: String = "".to_string();
        peers.iter().for_each(|x| arg += x);
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
    #[must_use]
    pub fn rpc_laddr(mut self, addr: &str) -> Self {
        self.process.set_arg("--rpc.laddr");
        self.process.set_arg(addr);
        self
    }

    /// ABCI listen address, or one of: 'kvstore', 'persistent_kvstore',
    /// 'counter', 'counter_serial' or 'noop' for local testing.
    /// Port required
    /// Default "tcp://127.0.0.1:26658"
    /// Sets the --proxy_app argument
    ///
    /// Compatible Commands:
    ///     start
    ///
    /// Note: Using this configuration command with incompatible terminating
    /// methods will cause the tendermint process to fail
    #[must_use]
    pub fn proxy_app(mut self, addr: &str) -> Self {
        self.process.set_arg("--proxy_app");
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn keep_addr_book(mut self) -> Self {
        self.process.set_arg("--keep_addr_book");
        self
    }

    fn apply_genesis(&self) {
        let genesis_bytes = match &self.genesis_bytes {
            Some(inner) => inner.clone(),
            None => {
                return;
            }
        };

        let target_path = self.home.join("config").join("genesis.json");
        let mut genesis_file = fs::File::create(target_path).unwrap();
        genesis_file.write_all(genesis_bytes.as_slice()).unwrap();
    }

    #[must_use]
    pub fn with_genesis(mut self, genesis_bytes: Vec<u8>) -> Self {
        self.genesis_bytes.replace(genesis_bytes);

        self
    }

    fn read_config_toml(&mut self) {
        let config_path = self.home.join("config/config.toml");
        let contents = fs::read_to_string(config_path).unwrap();
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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

    #[must_use]
    pub fn logs(mut self, show: bool) -> Self {
        self.show_logs = show;
        self
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub fn start(mut self) -> Self {
        self.install();
        self.mutate_configuration();
        self.process.set_arg("start");
        if !self.show_logs {
            self.process.command.stdout(Stdio::piped());
        }
        self.process.spawn().unwrap();
        if !self.show_logs {
            let stdout = self
                .process
                .process
                .as_mut()
                .unwrap()
                .stdout
                .take()
                .unwrap();
            std::thread::spawn(move || {
                let stdout = BufReader::new(stdout).lines();
                for line in stdout {
                    line.as_ref()
                        .unwrap()
                        .parse()
                        .map(|msg: LogMessage| {
                            log::debug!("{:#?}", msg);
                            match msg.message.as_str() {
                                "Started node" => log::info!("Started Tendermint"),
                                "executed block" => log::info!(
                                    "Executed block {}. txs={}",
                                    msg.meta[1].1,
                                    msg.meta[2].1
                                ),
                                "Applied snapshot chunk to ABCI app" => log::info!(
                                    "Verified state sync chunk {}/{}",
                                    msg.meta[3].1,
                                    msg.meta[4].1
                                ),
                                _ => {}
                            }
                        })
                        .unwrap_or_else(|_| println!("! {}", line.unwrap()));
                }
            });
        }
        self
    }

    /// Calls tendermint init with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    #[must_use]
    pub fn init(mut self) -> Self {
        self.install();
        self.process.set_arg("init");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
        self.mutate_configuration();

        self
    }

    pub fn kill(self) -> Result<()> {
        self.process.kill()
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub fn unsafe_reset_all(mut self) {
        self.install();
        self.process.set_arg("unsafe_reset_all");
        self.process.spawn().unwrap();
        self.process.wait().unwrap();
    }
}

#[derive(Debug)]
struct LogMessage {
    level: String,
    timestamp: String,
    message: String,
    meta: Vec<(String, String)>,
}

impl FromStr for LogMessage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        use nom::{
            branch::alt,
            bytes::complete::{tag, take, take_while_m_n},
            character::complete::{anychar, char, none_of},
            combinator::{cut, map, map_res},
            sequence::{delimited, preceded, terminated},
            IResult, Parser,
        };

        let into_string = |chars: Vec<char>| chars.into_iter().collect::<String>();

        let (s, level) = take::<_, _, nom::error::Error<_>>(1usize)(s)
            .map_err(|_| Error::App("Could not parse log line".to_string()))?;
        let (s, (date, time)) = separated_pair(
            preceded(
                char::<_, nom::error::Error<_>>('['),
                map(many1(none_of("|")), into_string),
            ),
            char('|'),
            terminated(map(many1(none_of("]")), into_string), char(']')),
        )(s)
        .map_err(|_| Error::App("Could not parse log line".to_string()))?;
        let (s, message) =
            preceded(char::<_, nom::error::Error<_>>(' '), take_until(" module="))(s)
                .map_err(|_| Error::App("Could not parse log line".to_string()))?;
        let (s, _) = many1::<_, _, nom::error::Error<_>, _>(tag(" "))(s)
            .map_err(|_| Error::App("Could not parse log line".to_string()))?;
        let (_, meta) = many1(preceded(
            many0(char(' ')),
            separated_pair(
                map(
                    many1(none_of::<_, _, nom::error::Error<_>>("=")),
                    into_string,
                ),
                char('='),
                alt((
                    map(
                        preceded(char('"'), terminated(many1(none_of("\"")), char('"'))),
                        into_string,
                    ),
                    map(terminated(many1(none_of(" ")), char(' ')), into_string),
                    map(
                        terminated(many1(none_of(" ")), nom::combinator::eof),
                        into_string,
                    ),
                )),
            ),
        ))(s)
        .map_err(|_| Error::App("Could not parse log line".to_string()))?;

        Ok(LogMessage {
            level: level.to_string(),
            timestamp: format!("{date} {time}"),
            message: message.trim().to_string(),
            meta,
        })
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
        let _ = Tendermint::new(temp_dir_path).stdout(Stdio::null()).init();

        let file_set: HashSet<String> = temp_dir_path
            .read_dir()
            .unwrap()
            .map(|x| x.unwrap().file_name().to_str().unwrap().to_string())
            .collect();

        let expected: HashSet<String> = HashSet::from([
            "config".to_string(),
            "data".to_string(),
            TENDERMINT_BINARY_NAME.to_string(),
        ]);

        assert_eq!(file_set, expected);
    }
}
