use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Deserializer};

use super::{BaseMap, LogLevel, Origin, Partial};
use crate::errors::{Error, Result};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    log: FileLog,
    #[serde(default)]
    web: FileWeb,
    #[serde(default)]
    node: FileNode,
    #[serde(default)]
    database: FileDatabase,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLog {
    #[serde(default, deserialize_with = "log_level")]
    level: Option<LogLevel>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileWeb {
    listen_address: Option<SocketAddr>,
    api_prefix: Option<String>,
    base_maps: Option<Vec<BaseMap>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileNode {
    host: Option<String>,
    http_port: Option<u16>,
    console_enabled: Option<bool>,
    console_port: Option<u16>,
    kiss_enabled: Option<bool>,
    kiss_port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDatabase {
    url: Option<String>,
}

fn log_level<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<LogLevel>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|s| s.parse::<LogLevel>().map_err(serde::de::Error::custom))
        .transpose()
}

fn tag<T>(value: Option<T>) -> Option<(T, Origin)> {
    value.map(|v| (v, Origin::File))
}

pub(super) fn parse(path: &Path, text: &str) -> Result<Partial> {
    let f: FileConfig = toml::from_str(text).map_err(|source| Error::ConfigParse {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    Ok(Partial {
        log_level: tag(f.log.level),
        web_listen_address: tag(f.web.listen_address),
        web_api_prefix: tag(f.web.api_prefix),
        web_base_maps: tag(f.web.base_maps),
        node_host: tag(f.node.host),
        node_http_port: tag(f.node.http_port),
        node_console_enabled: tag(f.node.console_enabled),
        node_console_port: tag(f.node.console_port),
        node_kiss_enabled: tag(f.node.kiss_enabled),
        node_kiss_port: tag(f.node.kiss_port),
        database_url: tag(f.database.url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Result<Partial> {
        parse(Path::new("t.toml"), text)
    }

    #[test]
    fn empty_file_sets_nothing() {
        assert_eq!(p("").unwrap(), Partial::default());
    }

    #[test]
    fn values_are_tagged_as_file() {
        let layer = p("[log]\nlevel = \"Debug\"\n[node]\nhost = \"h\"\nkiss_enabled = true\n").unwrap();
        assert_eq!(layer.log_level, Some((LogLevel::Debug, Origin::File)));
        assert_eq!(layer.node_host, Some(("h".into(), Origin::File)));
        assert_eq!(layer.node_kiss_enabled, Some((true, Origin::File)));
    }

    #[test]
    fn base_maps_as_array_of_tables() {
        let layer =
            p("[[web.base_maps]]\nname = \"A\"\nurl = \"https://a/{z}/{x}/{y}\"\nattribution = \"x\"\n").unwrap();
        let (maps, origin) = layer.web_base_maps.unwrap();
        assert_eq!((maps.len(), maps[0].max_zoom, origin), (1, 19, Origin::File));
        assert!(p("[[web.base_maps]]\nname = \"A\"\nurl = \"u\"\nattribution = \"x\"\nzoom = 3\n").is_err());
    }

    #[test]
    fn unknown_keys_are_refused() {
        assert!(p("[node]\nhots = \"h\"\n").is_err());
        assert!(p("[nodes]\n").is_err());
    }

    #[test]
    fn bad_values_are_refused() {
        assert!(p("[log]\nlevel = \"warn\"\n").is_err());
        assert!(p("[node]\nhttp_port = 70000\n").is_err());
        assert!(p("[web]\nlisten_address = \"nope\"\n").is_err());
    }
}
