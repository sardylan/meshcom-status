mod cli;
mod file;
mod render;

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub(crate) use cli::{CliInput, read_cli};

use crate::errors::{Error, Result};

const DEFAULT_CONFIG_PATH: &str = "meshcom-status.toml";
const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::Info;
const DEFAULT_API_PREFIX: &str = "/api";
const DEFAULT_HTTP_PORT: u16 = 80;
const DEFAULT_CONSOLE_ENABLED: bool = true;
const DEFAULT_CONSOLE_PORT: u16 = 2323;
const DEFAULT_KISS_ENABLED: bool = false;
const DEFAULT_KISS_PORT: u16 = 8001;
const DEFAULT_DATABASE_URL: &str = "postgres://meshcom:meshcom@localhost:5432/meshcom";

fn default_max_zoom() -> u8 {
    19
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaseMap {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) attribution: String,
    #[serde(default = "default_max_zoom")]
    pub(crate) max_zoom: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BaseMaps(pub(crate) Vec<BaseMap>);

impl FromStr for BaseMaps {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(BaseMaps(serde_json::from_str(s)?))
    }
}

fn default_base_maps() -> Vec<BaseMap> {
    let map = |name: &str, url: &str, attribution: &str, max_zoom: u8| BaseMap {
        name: name.into(),
        url: url.into(),
        attribution: attribution.into(),
        max_zoom,
    };
    vec![
        map(
            "OpenStreetMap",
            "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
            "© OpenStreetMap contributors",
            19,
        ),
        map(
            "OpenTopoMap",
            "https://tile.opentopomap.org/{z}/{x}/{y}.png",
            "Map data © OpenStreetMap contributors, SRTM · Style © OpenTopoMap (CC-BY-SA)",
            17,
        ),
        map(
            "Esri World Imagery",
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            "Tiles © Esri — Source: Esri, Maxar, Earthstar Geographics, and the GIS User Community",
            19,
        ),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    CommandLine,
    Environment,
    File,
    Default,
}

impl Origin {
    fn name(self) -> &'static str {
        match self {
            Origin::CommandLine => "command line",
            Origin::Environment => "environment",
            Origin::File => "configuration file",
            Origin::Default => "default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogLevel {
    Off,
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

impl FromStr for LogLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(LogLevel::Off),
            "error" => Ok(LogLevel::Error),
            "warning" => Ok(LogLevel::Warning),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(Error::Invalid {
                what: "log level",
                input: s.to_owned(),
                reason: "expected off, error, warning, info, debug or trace".into(),
            }),
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warning => "warning",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Partial {
    pub(crate) log_level: Option<(LogLevel, Origin)>,
    pub(crate) web_listen_address: Option<(SocketAddr, Origin)>,
    pub(crate) web_api_prefix: Option<(String, Origin)>,
    pub(crate) web_base_maps: Option<(Vec<BaseMap>, Origin)>,
    pub(crate) node_host: Option<(String, Origin)>,
    pub(crate) node_http_port: Option<(u16, Origin)>,
    pub(crate) node_console_enabled: Option<(bool, Origin)>,
    pub(crate) node_console_port: Option<(u16, Origin)>,
    pub(crate) node_kiss_enabled: Option<(bool, Origin)>,
    pub(crate) node_kiss_port: Option<(u16, Origin)>,
    pub(crate) database_url: Option<(String, Origin)>,
}

impl Partial {
    fn or(self, lower: Partial) -> Partial {
        Partial {
            log_level: self.log_level.or(lower.log_level),
            web_listen_address: self.web_listen_address.or(lower.web_listen_address),
            web_api_prefix: self.web_api_prefix.or(lower.web_api_prefix),
            web_base_maps: self.web_base_maps.or(lower.web_base_maps),
            node_host: self.node_host.or(lower.node_host),
            node_http_port: self.node_http_port.or(lower.node_http_port),
            node_console_enabled: self.node_console_enabled.or(lower.node_console_enabled),
            node_console_port: self.node_console_port.or(lower.node_console_port),
            node_kiss_enabled: self.node_kiss_enabled.or(lower.node_kiss_enabled),
            node_kiss_port: self.node_kiss_port.or(lower.node_kiss_port),
            database_url: self.database_url.or(lower.database_url),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogConfig {
    pub(crate) level: LogLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WebConfig {
    pub(crate) listen_address: SocketAddr,
    pub(crate) api_prefix: String,
    pub(crate) base_maps: Vec<BaseMap>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeConfig {
    pub(crate) host: Option<String>,
    pub(crate) http_port: u16,
    pub(crate) console_enabled: bool,
    pub(crate) console_port: u16,
    pub(crate) kiss_enabled: bool,
    pub(crate) kiss_port: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DatabaseConfig {
    pub(crate) url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Config {
    pub(crate) config_path: PathBuf,
    pub(crate) log: LogConfig,
    pub(crate) web: WebConfig,
    pub(crate) node: NodeConfig,
    pub(crate) database: DatabaseConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Run,
    SaveThenRun,
    DumpThenExit,
    Import(PathBuf),
    MigrateThenExit,
}

fn default_listen_address() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8080))
}

#[cfg(test)]
fn defaults_text() -> Vec<(&'static str, String)> {
    vec![
        ("config", DEFAULT_CONFIG_PATH.to_owned()),
        ("log_level", DEFAULT_LOG_LEVEL.to_string()),
        ("web_listen_address", default_listen_address().to_string()),
        ("web_api_prefix", DEFAULT_API_PREFIX.to_owned()),
        (
            "web_base_maps",
            default_base_maps()
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        ("node_host", "unset".to_owned()),
        ("node_http_port", DEFAULT_HTTP_PORT.to_string()),
        ("node_console_enabled", DEFAULT_CONSOLE_ENABLED.to_string()),
        ("node_console_port", DEFAULT_CONSOLE_PORT.to_string()),
        ("node_kiss_enabled", DEFAULT_KISS_ENABLED.to_string()),
        ("node_kiss_port", DEFAULT_KISS_PORT.to_string()),
        ("database_url", DEFAULT_DATABASE_URL.to_owned()),
    ]
}

fn invalid(setting: &'static str, origin: Origin, input: impl ToString, reason: &str) -> Error {
    Error::ConfigInvalid {
        setting,
        origin: origin.name(),
        input: input.to_string(),
        reason: reason.to_owned(),
    }
}

fn with_default<T>(value: Option<(T, Origin)>, default: T) -> (T, Origin) {
    value.unwrap_or((default, Origin::Default))
}

fn check_api_prefix((prefix, origin): (String, Origin)) -> Result<String> {
    let well_formed = prefix.len() > 1
        && prefix.starts_with('/')
        && !prefix.ends_with('/')
        && !prefix.contains("//")
        && prefix.chars().all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c));
    if well_formed {
        Ok(prefix)
    } else {
        Err(invalid(
            "web.api_prefix",
            origin,
            prefix,
            "must start with \"/\", not end with \"/\" and use only letters, digits, \"-\", \"_\", \".\"",
        ))
    }
}

fn check_base_maps((maps, origin): (Vec<BaseMap>, Origin)) -> Result<Vec<BaseMap>> {
    let bad = |input: &str, reason: &str| Err(invalid("web.base_maps", origin, input, reason));
    if maps.is_empty() {
        return bad("[]", "at least one base map is needed");
    }
    for (i, b) in maps.iter().enumerate() {
        if b.name.trim().is_empty() {
            return bad(&b.url, "every base map needs a name");
        }
        if maps[..i].iter().any(|o| o.name == b.name) {
            return bad(&b.name, "base map names must be unique");
        }
        let http = b.url.starts_with("https://") || b.url.starts_with("http://");
        if !http || !["{z}", "{x}", "{y}"].iter().all(|p| b.url.contains(p)) {
            return bad(&b.url, "the URL must be http(s) and contain {z}, {x} and {y}");
        }
        if !(1..=24).contains(&b.max_zoom) {
            return bad(&b.max_zoom.to_string(), "max_zoom must be between 1 and 24");
        }
    }
    Ok(maps)
}

fn check_host(value: Option<(String, Origin)>) -> Result<Option<String>> {
    value
        .map(|(host, origin)| {
            let ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c));
            if ok {
                Ok(host)
            } else {
                Err(invalid(
                    "node.host",
                    origin,
                    host,
                    "must be a host name or an IPv4 address",
                ))
            }
        })
        .transpose()
}

fn check_port(setting: &'static str, (port, origin): (u16, Origin)) -> Result<u16> {
    if port == 0 {
        Err(invalid(setting, origin, port, "must be between 1 and 65535"))
    } else {
        Ok(port)
    }
}

fn check_database_url((url, origin): (String, Origin)) -> Result<String> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        Ok(url)
    } else {
        Err(invalid(
            "database.url",
            origin,
            crate::store::redact(&url),
            "must start with postgres:// or postgresql://",
        ))
    }
}

fn build(config_path: PathBuf, p: Partial) -> Result<Config> {
    Ok(Config {
        config_path,
        log: LogConfig {
            level: with_default(p.log_level, DEFAULT_LOG_LEVEL).0,
        },
        web: WebConfig {
            listen_address: with_default(p.web_listen_address, default_listen_address()).0,
            api_prefix: check_api_prefix(with_default(p.web_api_prefix, DEFAULT_API_PREFIX.to_owned()))?,
            base_maps: check_base_maps(with_default(p.web_base_maps, default_base_maps()))?,
        },
        node: NodeConfig {
            host: check_host(p.node_host)?,
            http_port: check_port("node.http_port", with_default(p.node_http_port, DEFAULT_HTTP_PORT))?,
            console_enabled: with_default(p.node_console_enabled, DEFAULT_CONSOLE_ENABLED).0,
            console_port: check_port(
                "node.console_port",
                with_default(p.node_console_port, DEFAULT_CONSOLE_PORT),
            )?,
            kiss_enabled: with_default(p.node_kiss_enabled, DEFAULT_KISS_ENABLED).0,
            kiss_port: check_port("node.kiss_port", with_default(p.node_kiss_port, DEFAULT_KISS_PORT))?,
        },
        database: DatabaseConfig {
            url: check_database_url(with_default(p.database_url, DEFAULT_DATABASE_URL.to_owned()))?,
        },
    })
}

fn config_path(cli: &CliInput) -> PathBuf {
    cli.config_path()
        .map(|(p, _)| p)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn resolve_from(cli_layer: Partial, path: PathBuf, file_text: Option<&str>) -> Result<(Config, Vec<String>)> {
    let (file_layer, notes) = match file_text {
        Some(text) => (
            file::parse(&path, text)?,
            vec![format!("configuration file {} loaded", path.display())],
        ),
        None => (
            Partial::default(),
            vec![format!(
                "configuration file {} not found, using defaults",
                path.display()
            )],
        ),
    };
    build(path, cli_layer.or(file_layer)).map(|config| (config, notes))
}

fn read_file(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::ConfigRead {
            path: path.to_owned(),
            source,
        }),
    }
}

pub(crate) fn resolve(cli: &CliInput) -> Result<(Config, Vec<String>)> {
    let path = config_path(cli);
    let text = read_file(&path)?;
    resolve_from(cli.layer(), path, text.as_deref())
}

pub(crate) fn action(cli: &CliInput) -> Action {
    let c = &cli.cli;
    match (&c.import, c.db_migrate, c.config_save, c.config_dump) {
        (Some(path), _, _, _) => Action::Import(path.clone()),
        (None, true, _, _) => Action::MigrateThenExit,
        (None, false, _, true) => Action::DumpThenExit,
        (None, false, true, false) => Action::SaveThenRun,
        (None, false, false, false) => Action::Run,
    }
}

pub(crate) fn write_file(config: &Config) -> Result<()> {
    let text = render::render(config, &cli::short_help);
    let path = &config.config_path;
    let wrap = |source| Error::ConfigWrite {
        path: path.clone(),
        source,
    };
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::fs::create_dir_all)
        .transpose()
        .map_err(wrap)?;
    std::fs::write(path, text).map_err(wrap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Config {
        build(PathBuf::from("x.toml"), Partial::default()).unwrap()
    }

    #[test]
    fn runs_with_no_sources() {
        let c = defaults();
        assert_eq!(c.log.level, LogLevel::Info);
        assert_eq!(c.web.listen_address, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(c.web.api_prefix, "/api");
        assert_eq!(c.node.host, None);
        assert!(c.node.console_enabled);
        assert!(!c.node.kiss_enabled);
        assert_eq!(c.database.url, "postgres://meshcom:meshcom@localhost:5432/meshcom");
    }

    #[test]
    fn precedence_cli_over_file_over_default() {
        let cli = Partial {
            node_http_port: Some((81, Origin::CommandLine)),
            ..Partial::default()
        };
        let file = "[node]\nhttp_port = 82\nkiss_port = 9001\n";
        let (c, notes) = resolve_from(cli, PathBuf::from("a.toml"), Some(file)).unwrap();
        assert_eq!(c.node.http_port, 81);
        assert_eq!(c.node.kiss_port, 9001);
        assert_eq!(c.node.console_port, 2323);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn missing_file_is_a_note_not_an_error() {
        let (_, notes) = resolve_from(Partial::default(), PathBuf::from("a.toml"), None).unwrap();
        assert!(notes[0].contains("not found"));
    }

    #[test]
    fn invalid_value_names_setting_and_origin() {
        let cli = Partial {
            web_api_prefix: Some(("api/".into(), Origin::Environment)),
            ..Partial::default()
        };
        let err = build(PathBuf::from("a"), cli).unwrap_err().to_string();
        assert!(err.contains("web.api_prefix") && err.contains("environment"), "{err}");
    }

    #[test]
    fn api_prefix_rules() {
        let check = |s: &str| check_api_prefix((s.into(), Origin::File)).is_ok();
        assert!(check("/api"));
        assert!(check("/meshcom/api"));
        assert!(!check("/"));
        assert!(!check("api"));
        assert!(!check("/api/"));
        assert!(!check("/a//b"));
        assert!(!check("/a?b"));
    }

    #[test]
    fn other_checks() {
        let bm = |name: &str, url: &str, max_zoom: u8| BaseMap {
            name: name.into(),
            url: url.into(),
            attribution: String::new(),
            max_zoom,
        };
        let check = |maps: Vec<BaseMap>| check_base_maps((maps, Origin::File));
        assert!(check(default_base_maps()).is_ok());
        assert!(check(vec![]).is_err());
        assert!(check(vec![bm("A", "https://x/{z}/{x}.png", 19)]).is_err());
        assert!(check(vec![bm("A", "ftp://x/{z}/{x}/{y}", 19)]).is_err());
        assert!(check(vec![bm("", "https://x/{z}/{x}/{y}", 19)]).is_err());
        assert!(check(vec![bm("A", "https://x/{z}/{x}/{y}", 0)]).is_err());
        assert!(
            check(vec![
                bm("A", "https://x/{z}/{x}/{y}", 19),
                bm("A", "https://y/{z}/{x}/{y}", 19)
            ])
            .is_err()
        );
        assert_eq!(
            "[{\"name\":\"A\",\"url\":\"https://x/{z}/{x}/{y}\",\"attribution\":\"\"}]"
                .parse::<BaseMaps>()
                .unwrap()
                .0[0]
                .max_zoom,
            19
        );
        assert!("nope".parse::<BaseMaps>().is_err());
        assert!(check_host(Some(("a b".into(), Origin::File))).is_err());
        assert_eq!(
            check_host(Some(("node-1.lan".into(), Origin::File))).unwrap(),
            Some("node-1.lan".into())
        );
        assert!(check_port("p", (0, Origin::File)).is_err());
        assert!(check_database_url(("mysql://a".into(), Origin::File)).is_err());
        assert!(check_database_url(("postgresql://a/b".into(), Origin::File)).is_ok());
    }

    #[test]
    fn log_level_names() {
        assert_eq!("WARNING".parse::<LogLevel>().unwrap(), LogLevel::Warning);
        assert_eq!("Info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert!("warn".parse::<LogLevel>().is_err());
        assert!("3".parse::<LogLevel>().is_err());
        assert_eq!(LogLevel::Warning.to_string(), "warning");
    }

    #[test]
    fn actions() {
        assert_eq!(action(&cli::parse_from(&["x"])), Action::Run);
        assert_eq!(action(&cli::parse_from(&["x", "--config-save"])), Action::SaveThenRun);
        assert_eq!(action(&cli::parse_from(&["x", "--config-dump"])), Action::DumpThenExit);
        assert_eq!(
            action(&cli::parse_from(&["x", "--import", "e.json"])),
            Action::Import(PathBuf::from("e.json"))
        );
        assert_eq!(
            action(&cli::parse_from(&["x", "--db-migrate"])),
            Action::MigrateThenExit
        );
    }

    #[test]
    fn config_path_from_cli() {
        assert_eq!(
            config_path(&cli::parse_from(&["x", "-c", "/tmp/a.toml"])),
            PathBuf::from("/tmp/a.toml")
        );
        assert_eq!(
            config_path(&cli::parse_from(&["x"])),
            PathBuf::from(DEFAULT_CONFIG_PATH)
        );
    }

    #[test]
    fn rendered_file_reads_back_to_the_same_configuration() {
        let custom = Partial {
            node_host: Some(("10.1.2.3".into(), Origin::CommandLine)),
            node_kiss_enabled: Some((true, Origin::CommandLine)),
            web_base_maps: Some((
                vec![BaseMap {
                    name: "Topo \"quoted\"".into(),
                    url: "https://t.example/{z}/{x}/{y}.png".into(),
                    attribution: "© a \\ b".into(),
                    max_zoom: 17,
                }],
                Origin::CommandLine,
            )),
            ..Partial::default()
        };
        [Partial::default(), custom].into_iter().for_each(|layer| {
            let original = build(PathBuf::from("a.toml"), layer).unwrap();
            let text = render::render(&original, &cli::short_help);
            let (back, _) = resolve_from(Partial::default(), PathBuf::from("a.toml"), Some(&text)).unwrap();
            assert_eq!(back, original, "{text}");
        });
    }

    #[test]
    fn shipped_default_config_matches_the_defaults() {
        let shipped = include_str!("../../packaging/meshcom-status.toml");
        assert_eq!(render::render(&defaults(), &cli::short_help), shipped);
        let (parsed, _) = resolve_from(Partial::default(), PathBuf::from("x.toml"), Some(shipped)).unwrap();
        assert_eq!(parsed, defaults());
    }

    #[test]
    fn write_file_creates_parent_folders() {
        let dir = std::env::temp_dir().join(format!("meshcom-status-test-{}", std::process::id()));
        let path = dir.join("sub").join("c.toml");
        let config = Config {
            config_path: path.clone(),
            ..defaults()
        };
        write_file(&config).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[node]"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
