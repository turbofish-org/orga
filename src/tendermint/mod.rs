pub mod client;

use crate::error::{Error, Result};
use flate2::read::GzDecoder;
use hex_literal::hex;
use is_executable::IsExecutable;
use log::{debug, info};
use nom::bytes::complete::take_until;
use nom::multi::{many0, many1};
use nom::sequence::separated_pair;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{prelude::*, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use tar::Archive;
use toml_edit::{value, DocumentMut};

#[cfg(target_os = "macos")]
static TENDERMINT_BINARY_URL: &str = "https://github.com/informalsystems/tendermint/releases/download/v0.34.26/tendermint_0.34.26_darwin_amd64.tar.gz";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/informalsystems/tendermint/releases/download/v0.34.26/tendermint_0.34.26_linux_amd64.tar.gz";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
static TENDERMINT_BINARY_URL: &str = "https://github.com/informalsystems/tendermint/releases/download/v0.34.26/tendermint_0.34.26_linux_arm64.tar.gz";

#[cfg(target_os = "macos")]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("39dfde6ccc2c8b4cb699d1f3788b97da16cc8495156c39c82e94fa3834187909");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("70415c1d20f48e4c19d8317ec7befd924681bb2d144ad8fded429041b80b3f79");
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
static TENDERMINT_ZIP_HASH: [u8; 32] =
    hex!("b0c9b5fae8a7dc53d84d62867204927ef37b1f91be5617f33a8f7fe378dfc5b9");

const TENDERMINT_BINARY_NAME: &str = "tendermint-v0.34.26";

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

/// Tendermint child process handle.
pub struct Child {
    child: std::process::Child,
    sender: Sender<Option<()>>,
}

impl Child {
    /// Create a new Tendermint child process handle.
    pub fn new(child: std::process::Child, sender: Sender<Option<()>>) -> Self {
        Self { child, sender }
    }

    /// Kill the child process.
    pub fn kill(&mut self) -> Result<()> {
        let _ = self.sender.send(Some(()));
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }
}

/// Tendermint process manager.
#[derive(Debug)]
pub struct Tendermint {
    command: std::process::Command,
    home: PathBuf,
    genesis_bytes: Option<Vec<u8>>,
    config_contents: Option<toml_edit::DocumentMut>,
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
            command: Command::new(tm_bin_path.to_str().unwrap()),
            home: home_path.clone().into(),
            genesis_bytes: None,
            config_contents: None,
            show_logs: false,
        };
        tendermint.home(home_path.into())
    }

    async fn install(&self) {
        let tendermint_path = self.home.join(TENDERMINT_BINARY_NAME);

        if tendermint_path.is_executable() {
            debug!("Tendermint already installed");
            return;
        }

        info!("Installing Tendermint to {}", self.home.to_str().unwrap());
        let buf = reqwest::get(TENDERMINT_BINARY_URL)
            .await
            .expect("Failed to download Tendermint zip file from GitHub")
            .bytes()
            .await
            .expect("Failed to read bytes from Tendermint zip file")
            .to_vec();

        verify_hash(&buf);

        let cursor = std::io::Cursor::new(buf);
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

    /// Sets command line flags for the Tendermint process.
    pub fn flags(mut self, flags: Vec<String>) -> Self {
        for flag in flags {
            self.command.arg(flag.trim());
        }
        self
    }

    fn home(mut self, home_path: PathBuf) -> Self {
        let new_home = home_path.to_str().unwrap();
        self.command.arg("--home");
        self.command.arg(new_home);
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
        self.command.arg("--log_level");
        self.command.arg(level);
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
        self.command.arg("--trace");
        self
    }

    /// Node name
    /// Sets the --moniker argument
    ///
    /// Compatible Commands:
    ///     start
    #[must_use]
    pub fn moniker(mut self, moniker: &str) -> Self {
        self.command.arg("--moniker");
        self.command.arg(moniker);
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
        self.command.arg("--p2p.laddr");
        self.command.arg(addr);
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
        self.command.arg("--p2p.persistent_peers");
        let mut arg: String = "".to_string();
        peers.iter().for_each(|x| arg += x);
        self.command.arg(&arg);
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
        self.command.arg("--rpc.laddr");
        self.command.arg(addr);
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
        self.command.arg("--proxy_app");
        self.command.arg(addr);
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
        self.command.stdout(cfg);
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
        self.command.stderr(cfg);
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
        self.command.arg("--keep_addr_book");
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

    /// Sets the genesis to use for the Tendermint process from the bytes of the
    /// `genesis.json`.
    #[must_use]
    pub fn with_genesis(mut self, genesis_bytes: Vec<u8>) -> Self {
        self.genesis_bytes.replace(genesis_bytes);

        self
    }

    fn read_config_toml(&mut self) {
        let config_path = self.home.join("config/config.toml");
        let contents = fs::read_to_string(config_path).unwrap();
        let document = contents
            .parse::<DocumentMut>()
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

    /// Edits the block time located in the config.toml in the config directory
    /// under the tendermint home
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

    /// Enable or disable Tendermint log display.
    #[must_use]
    pub fn logs(mut self, show: bool) -> Self {
        self.show_logs = show;
        self
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub async fn start(mut self) -> Child {
        self.install().await;
        self.mutate_configuration();
        self.command.arg("start");
        if !self.show_logs {
            self.command.stdout(Stdio::piped());
        }

        let mut child = self.command.spawn().unwrap();

        let (tx, rx): (Sender<Option<()>>, Receiver<Option<()>>) = mpsc::channel();
        if !self.show_logs {
            let stdout = child.stdout.take().unwrap();

            std::thread::spawn(move || {
                let mut stdout = BufReader::new(stdout);
                let mut line = String::new();

                loop {
                    if let Ok(Some(_)) = rx.try_recv() {
                        break;
                    }

                    stdout.read_line(&mut line).unwrap();
                    line.parse()
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
                                _ if msg.level == "E" => {
                                    let module = msg
                                        .meta
                                        .iter()
                                        .find(|(k, _)| *k == "module")
                                        .map(|(_, v)| v.clone())
                                        .unwrap();
                                    if module != "p2p" && module != "rpc" {
                                        log::error!(
                                            "Tendermint error: {} {:?}",
                                            msg.message,
                                            msg.meta
                                        )
                                    }
                                }
                                _ => {}
                            }
                        })
                        .unwrap_or_else(|_| println!("! {}", line));

                    line.clear();
                }
            });
        }

        Child::new(child, tx)
    }

    /// Calls tendermint init with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    #[must_use]
    pub async fn init(mut self) -> Self {
        self.install().await;
        self.command.arg("init");
        let mut child = self.command.spawn().unwrap();
        child.wait().unwrap();
        self.mutate_configuration();

        self
    }

    /// Calls tendermint start with configured arguments
    ///
    /// Note: This will locally install the Tendermint binary if it is
    /// not already contained in the Tendermint home directory
    pub async fn unsafe_reset_all(mut self) {
        self.install().await;
        self.command.arg("unsafe_reset_all");
        let mut child = self.command.spawn().unwrap();
        child.wait().unwrap();
    }
}

#[derive(Debug)]
struct LogMessage {
    level: String,
    message: String,
    meta: Vec<(String, String)>,
}

impl FromStr for LogMessage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        use nom::{
            branch::alt,
            bytes::complete::{tag, take},
            character::complete::{char, none_of},
            combinator::map,
            sequence::{preceded, terminated},
        };

        let into_string = |chars: Vec<char>| chars.into_iter().collect::<String>();

        let (s, level) = take::<_, _, nom::error::Error<_>>(1usize)(s)
            .map_err(|_| Error::App("Could not parse log line".to_string()))?;
        let (s, _) = separated_pair(
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
            message: message.trim().to_string(),
            meta,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    #[ignore]
    fn tendermint_init() {
        let temp_dir = TempDir::new().unwrap();
        let temp_dir_path = temp_dir.path();
        // let _ = Tendermint::new(temp_dir_path).stdout(Stdio::null()).init();

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
