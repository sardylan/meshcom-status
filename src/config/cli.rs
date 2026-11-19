use std::net::SocketAddr;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::parser::ValueSource;
use clap::{ArgAction, ArgMatches, CommandFactory, FromArgMatches, Parser};

use super::{BaseMaps, LogLevel, Origin, Partial};
use crate::errors::{Error, Result};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default());

const H_CONFIG: &str = "⚙️  Configuration";
const H_ACTIONS: &str = "🛠️  Actions";
const H_LOG: &str = "📜 Logging";
const H_WEB: &str = "🌐 Web";
const H_NODE: &str = "📡 Node";
const H_DATABASE: &str = "🗄️  Database";

const EXAMPLES: &str = "📖 Examples:
  meshcom-status --node-host 192.0.2.10
  meshcom-status -n 192.0.2.10 --web-listen-address 127.0.0.1:8080 --log-level debug
  meshcom-status -n 192.0.2.10 --config-dump";

/// 🗺️  Status server for a MeshCom network: shows on a live map which nodes hear which.
///
/// The server connects to one MeshCom node, reads every frame the node receives (from its net
/// console and/or its KISS port), and builds a graph of who heard whom. A web page shows the graph
/// on a map and is updated live through a WebSocket.
///
/// Settings come from the command line, then environment variables, then the TOML configuration
/// file, then built-in defaults: the first source that sets a value wins.
#[derive(Debug, Parser)]
#[command(version, styles = STYLES, after_long_help = EXAMPLES)]
pub(crate) struct Cli {
    /// Path of the TOML configuration file.
    ///
    /// Settings on the command line and in environment variables override the ones in this file.
    /// A missing file is not an error. Default: meshcom-status.toml
    #[arg(short, long, value_name = "PATH", env = "MESHCOM_STATUS_CONFIG", help_heading = H_CONFIG)]
    pub(crate) config: Option<PathBuf>,

    /// Write the configuration file, then start.
    ///
    /// Resolves the configuration from every source, writes it to the configuration file path
    /// (replacing the file), then starts normally.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["config_dump", "import", "db_migrate"], help_heading = H_ACTIONS)]
    pub(crate) config_save: bool,

    /// Write the configuration file, then exit.
    ///
    /// Resolves the configuration from every source, writes it to the configuration file path
    /// (replacing the file), then exits without starting anything.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["import", "db_migrate"], help_heading = H_ACTIONS)]
    pub(crate) config_dump: bool,

    /// Import an export of another instance into the database, then exit.
    ///
    /// The file is a JSON object with the "nodes" array of GET /api/public/v1/nodes and the
    /// "frames" array of GET /api/public/v1/frames, both optional. Receptions are added; nodes
    /// are merged with the stored ones.
    #[arg(long, value_name = "FILE", conflicts_with = "db_migrate", help_heading = H_ACTIONS)]
    pub(crate) import: Option<PathBuf>,

    /// Apply the pending database migrations, then exit.
    ///
    /// Connects to the database, brings its schema up to date and exits without starting
    /// anything. The server applies the migrations at startup too; this lets a deployment do it as
    /// a separate step.
    #[arg(long, action = ArgAction::SetTrue, help_heading = H_ACTIONS)]
    pub(crate) db_migrate: bool,

    /// Log level.
    ///
    /// One of off, error, warning, info, debug, trace (any letter case). A level shows its own
    /// messages and every more severe one. Default: info
    #[arg(long, value_name = "LEVEL", env = "MESHCOM_STATUS_LOG_LEVEL", help_heading = H_LOG)]
    pub(crate) log_level: Option<LogLevel>,

    /// Address and port the web server listens on.
    ///
    /// Use 127.0.0.1 to accept local connections only. Default: 0.0.0.0:8080
    #[arg(long, value_name = "ADDR", env = "MESHCOM_STATUS_WEB_LISTEN_ADDRESS", help_heading = H_WEB)]
    pub(crate) web_listen_address: Option<SocketAddr>,

    /// Path prefix of every API endpoint.
    ///
    /// Starts with "/" and does not end with "/". Change it when the server sits behind a reverse
    /// proxy under another path. Default: /api
    #[arg(long, value_name = "PREFIX", env = "MESHCOM_STATUS_WEB_API_PREFIX", help_heading = H_WEB)]
    pub(crate) web_api_prefix: Option<String>,

    /// Base maps the page offers.
    ///
    /// A JSON array of {"name", "url", "attribution", "max_zoom"}; "url" is a raster tile template
    /// with {z}, {x} and {y}, loaded by the browser. The first one is shown by default. In the
    /// configuration file, one [[web.base_maps]] table per base map.
    /// Default: OpenStreetMap, OpenTopoMap, Esri World Imagery
    #[arg(long, value_name = "JSON", env = "MESHCOM_STATUS_WEB_BASE_MAPS", help_heading = H_WEB)]
    pub(crate) web_base_maps: Option<BaseMaps>,

    /// Host name or IP address of the MeshCom node.
    ///
    /// When unset, no node is contacted and the map stays empty. Default: unset
    #[arg(short = 'n', long, value_name = "HOST", env = "MESHCOM_STATUS_NODE_HOST", help_heading = H_NODE)]
    pub(crate) node_host: Option<String>,

    /// HTTP port of the node.
    ///
    /// Used to read the node's own callsign and position. Default: 80
    #[arg(long, value_name = "PORT", env = "MESHCOM_STATUS_NODE_HTTP_PORT", help_heading = H_NODE)]
    pub(crate) node_http_port: Option<u16>,

    /// Read frames from the node's net console.
    ///
    /// The net console gives every received frame, HEY beacons and duplicates included, with
    /// signal reports. The node needs "--netconsole on" and "--loradebug on". Default: true
    #[arg(long, value_name = "BOOL", action = ArgAction::Set, env = "MESHCOM_STATUS_NODE_CONSOLE_ENABLED", help_heading = H_NODE)]
    pub(crate) node_console_enabled: Option<bool>,

    /// TCP port of the node's net console.
    ///
    /// Default: 2323
    #[arg(long, value_name = "PORT", env = "MESHCOM_STATUS_NODE_CONSOLE_PORT", help_heading = H_NODE)]
    pub(crate) node_console_port: Option<u16>,

    /// Read frames from the node's KISS port.
    ///
    /// KISS gives positions and messages, first copy only, without HEY beacons. Enabling it
    /// together with the net console counts the same frame twice. Default: false
    #[arg(long, value_name = "BOOL", action = ArgAction::Set, env = "MESHCOM_STATUS_NODE_KISS_ENABLED", help_heading = H_NODE)]
    pub(crate) node_kiss_enabled: Option<bool>,

    /// TCP port of the node's KISS interface.
    ///
    /// Default: 8001
    #[arg(long, value_name = "PORT", env = "MESHCOM_STATUS_NODE_KISS_PORT", help_heading = H_NODE)]
    pub(crate) node_kiss_port: Option<u16>,

    /// PostgreSQL connection URL.
    ///
    /// Every reception and the state of every node are stored there, forever. The schema is
    /// created and updated at startup. The password is never logged.
    /// Default: postgres://meshcom:meshcom@localhost:5432/meshcom
    #[arg(long, value_name = "URL", env = "MESHCOM_STATUS_DATABASE_URL", help_heading = H_DATABASE)]
    pub(crate) database_url: Option<String>,
}

pub(crate) struct CliInput {
    pub(crate) cli: Cli,
    matches: ArgMatches,
}

impl CliInput {
    fn origin(&self, id: &str) -> Option<Origin> {
        match self.matches.value_source(id) {
            Some(ValueSource::CommandLine) => Some(Origin::CommandLine),
            Some(ValueSource::EnvVariable) => Some(Origin::Environment),
            _ => None,
        }
    }

    fn tag<T>(&self, id: &str, value: Option<T>) -> Option<(T, Origin)> {
        value.zip(self.origin(id))
    }

    pub(crate) fn config_path(&self) -> Option<(PathBuf, Origin)> {
        self.tag("config", self.cli.config.clone())
    }

    pub(crate) fn layer(&self) -> Partial {
        let c = &self.cli;
        Partial {
            log_level: self.tag("log_level", c.log_level),
            web_listen_address: self.tag("web_listen_address", c.web_listen_address),
            web_api_prefix: self.tag("web_api_prefix", c.web_api_prefix.clone()),
            web_base_maps: self.tag("web_base_maps", c.web_base_maps.clone().map(|b| b.0)),
            node_host: self.tag("node_host", c.node_host.clone()),
            node_http_port: self.tag("node_http_port", c.node_http_port),
            node_console_enabled: self.tag("node_console_enabled", c.node_console_enabled),
            node_console_port: self.tag("node_console_port", c.node_console_port),
            node_kiss_enabled: self.tag("node_kiss_enabled", c.node_kiss_enabled),
            node_kiss_port: self.tag("node_kiss_port", c.node_kiss_port),
            database_url: self.tag("database_url", c.database_url.clone()),
        }
    }
}

pub(crate) fn read_cli() -> Result<Option<CliInput>> {
    match Cli::command().try_get_matches() {
        Ok(matches) => parse_matches(matches).map(Some),
        Err(e) => {
            // Printing is the purpose here: this is clap's own help, version or usage text.
            let _ = e.print();
            if e.use_stderr() { Err(Error::Cli(e)) } else { Ok(None) }
        }
    }
}

fn parse_matches(matches: ArgMatches) -> Result<CliInput> {
    let cli = Cli::from_arg_matches(&matches)?;
    Ok(CliInput { cli, matches })
}

pub(crate) fn short_help(id: &str) -> String {
    Cli::command()
        .get_arguments()
        .find(|a| a.get_id() == id)
        .and_then(|a| a.get_help())
        .map(|h| h.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn parse_from(args: &[&str]) -> CliInput {
    let matches = Cli::command().try_get_matches_from(args).expect("valid test arguments");
    parse_matches(matches).expect("valid test arguments")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn command_line_values_are_tagged() {
        let input = parse_from(&[
            "x",
            "--node-host",
            "10.0.0.1",
            "--node-kiss-enabled",
            "true",
            "--log-level",
            "DEBUG",
        ]);
        let layer = input.layer();
        assert_eq!(layer.node_host, Some(("10.0.0.1".into(), Origin::CommandLine)));
        assert_eq!(layer.node_kiss_enabled, Some((true, Origin::CommandLine)));
        assert_eq!(layer.log_level, Some((LogLevel::Debug, Origin::CommandLine)));
        assert_eq!(layer.web_listen_address, None);
    }

    #[test]
    fn actions_exclude_each_other() {
        let pairs = [
            ["--config-save", "--config-dump"],
            ["--config-save", "--db-migrate"],
            ["--config-dump", "--db-migrate"],
            ["--import=a", "--db-migrate"],
            ["--import=a", "--config-save"],
            ["--import=a", "--config-dump"],
        ];
        pairs.iter().for_each(|[a, b]| {
            assert!(Cli::command().try_get_matches_from(["x", a, b]).is_err(), "{a} {b}");
        });
    }

    #[test]
    fn bad_log_level_is_refused() {
        assert!(
            Cli::command()
                .try_get_matches_from(["x", "--log-level", "warn"])
                .is_err()
        );
    }

    #[test]
    fn long_help_states_every_default() {
        let defaults = super::super::defaults_text();
        let cmd = Cli::command();
        defaults.iter().for_each(|(id, text)| {
            let arg = cmd.get_arguments().find(|a| a.get_id() == *id).expect(id);
            let long = arg.get_long_help().map(|h| h.to_string()).unwrap_or_default();
            assert!(long.contains(&format!("Default: {text}")), "{id}: {long}");
        });
    }

    #[test]
    fn short_help_is_the_first_line() {
        assert_eq!(short_help("node_host"), "Host name or IP address of the MeshCom node");
        assert_eq!(short_help("nope"), "");
    }
}
