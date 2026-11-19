use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    Int(ParseIntError),
    Float(ParseFloatError),
    Logging(tracing::subscriber::SetGlobalDefaultError),
    Cli(clap::Error),
    Database(sqlx::Error),
    Migrate(sqlx::migrate::MigrateError),

    ConfigRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ConfigParse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    ConfigWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    Runtime(std::io::Error),
    Bind {
        address: String,
        source: std::io::Error,
    },
    Serve(std::io::Error),
    DatabaseConnect {
        url: String,
        source: sqlx::Error,
    },
    Import {
        path: PathBuf,
        source: Box<Error>,
    },
    Connect {
        what: &'static str,
        address: String,
        source: std::io::Error,
    },
    Encoding {
        path: String,
        encoding: &'static str,
        source: std::io::Error,
    },

    ConfigInvalid {
        setting: &'static str,
        origin: &'static str,
        input: String,
        reason: String,
    },
    Invalid {
        what: &'static str,
        input: String,
        reason: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
            Error::Int(e) => write!(f, "invalid integer: {e}"),
            Error::Float(e) => write!(f, "invalid number: {e}"),
            Error::Logging(e) => write!(f, "cannot start logging: {e}"),
            Error::Cli(e) => write!(f, "invalid command line ({})", e.kind()),
            Error::Database(e) => write!(f, "database error: {e}"),
            Error::Migrate(e) => write!(f, "cannot apply the database migrations: {e}"),
            Error::DatabaseConnect { url, source } => write!(f, "cannot connect to the database at {url}: {source}"),
            Error::Import { path, source } => write!(f, "cannot import {}: {source}", path.display()),
            Error::ConfigRead { path, source } => {
                write!(f, "cannot read configuration file {}: {source}", path.display())
            }
            Error::ConfigParse { path, source } => {
                write!(f, "cannot parse configuration file {}: {source}", path.display())
            }
            Error::ConfigWrite { path, source } => {
                write!(f, "cannot write configuration file {}: {source}", path.display())
            }
            Error::Runtime(e) => write!(f, "cannot start the async runtime: {e}"),
            Error::Bind { address, source } => write!(f, "cannot listen on {address}: {source}"),
            Error::Serve(e) => write!(f, "web server failed: {e}"),
            Error::Connect { what, address, source } => {
                write!(f, "cannot connect to {what} at {address}: {source}")
            }
            Error::Encoding { path, encoding, source } => {
                write!(f, "cannot make the {encoding} form of {path}: {source}")
            }
            Error::ConfigInvalid {
                setting,
                origin,
                input,
                reason,
            } => {
                write!(
                    f,
                    "invalid value {input:?} for setting {setting} (from {origin}): {reason}"
                )
            }
            Error::Invalid { what, input, reason } => write!(f, "invalid {what} {input:?}: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Int(e) => Some(e),
            Error::Float(e) => Some(e),
            Error::Logging(e) => Some(e),
            Error::Cli(e) => Some(e),
            Error::Database(e) => Some(e),
            Error::Migrate(e) => Some(e),
            Error::DatabaseConnect { source, .. } => Some(source),
            Error::Import { source, .. } => Some(source.as_ref()),
            Error::ConfigRead { source, .. } => Some(source),
            Error::ConfigParse { source, .. } => Some(source.as_ref()),
            Error::ConfigWrite { source, .. } => Some(source),
            Error::Runtime(e) => Some(e),
            Error::Bind { source, .. } => Some(source),
            Error::Serve(e) => Some(e),
            Error::Connect { source, .. } => Some(source),
            Error::Encoding { source, .. } => Some(source),
            Error::ConfigInvalid { .. } | Error::Invalid { .. } => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::Int(e)
    }
}

impl From<ParseFloatError> for Error {
    fn from(e: ParseFloatError) -> Self {
        Error::Float(e)
    }
}

impl From<tracing::subscriber::SetGlobalDefaultError> for Error {
    fn from(e: tracing::subscriber::SetGlobalDefaultError) -> Self {
        Error::Logging(e)
    }
}

impl From<clap::Error> for Error {
    fn from(e: clap::Error) -> Self {
        Error::Cli(e)
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Database(e)
    }
}

impl From<sqlx::migrate::MigrateError> for Error {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Error::Migrate(e)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    fn io() -> std::io::Error {
        std::io::Error::other("boom")
    }

    fn toml_error() -> toml::de::Error {
        toml::from_str::<toml::Table>("= nope").unwrap_err()
    }

    fn json_error() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("{").unwrap_err()
    }

    fn logging_error() -> tracing::subscriber::SetGlobalDefaultError {
        let _ = tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default());
        tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default()).unwrap_err()
    }

    fn clap_error() -> clap::Error {
        clap::Error::new(clap::error::ErrorKind::UnknownArgument)
    }

    fn all() -> Vec<Error> {
        vec![
            Error::Io(io()),
            Error::Json(json_error()),
            Error::Int("x".parse::<u8>().unwrap_err()),
            Error::Float("x".parse::<f64>().unwrap_err()),
            Error::Logging(logging_error()),
            Error::Cli(clap_error()),
            Error::Database(sqlx::Error::RowNotFound),
            Error::Migrate(sqlx::migrate::MigrateError::VersionMissing(1)),
            Error::DatabaseConnect {
                url: "postgres://x".into(),
                source: sqlx::Error::PoolTimedOut,
            },
            Error::Import {
                path: "a.json".into(),
                source: Box::new(Error::Io(io())),
            },
            Error::ConfigRead {
                path: "a.toml".into(),
                source: io(),
            },
            Error::ConfigParse {
                path: "a.toml".into(),
                source: Box::new(toml_error()),
            },
            Error::ConfigWrite {
                path: "a.toml".into(),
                source: io(),
            },
            Error::Runtime(io()),
            Error::Bind {
                address: "0.0.0.0:1".into(),
                source: io(),
            },
            Error::Serve(io()),
            Error::Connect {
                what: "KISS",
                address: "h:1".into(),
                source: io(),
            },
            Error::Encoding {
                path: "index.html".into(),
                encoding: "br",
                source: io(),
            },
            Error::ConfigInvalid {
                setting: "web.api_prefix",
                origin: "file",
                input: "x".into(),
                reason: "r".into(),
            },
            Error::Invalid {
                what: "frame",
                input: "x".into(),
                reason: "r".into(),
            },
        ]
    }

    #[test]
    fn display_is_never_empty() {
        all().iter().for_each(|e| assert!(!e.to_string().is_empty(), "{e:?}"));
    }

    #[test]
    fn source_is_set_for_wrapping_variants() {
        all().iter().for_each(|e| {
            let expected = !matches!(e, Error::ConfigInvalid { .. } | Error::Invalid { .. });
            assert_eq!(e.source().is_some(), expected, "{e:?}");
        });
    }

    #[test]
    fn from_conversions() {
        assert!(matches!(Error::from(io()), Error::Io(_)));
        assert!(matches!(Error::from(json_error()), Error::Json(_)));
        assert!(matches!(Error::from("x".parse::<u8>().unwrap_err()), Error::Int(_)));
        assert!(matches!(Error::from("x".parse::<f64>().unwrap_err()), Error::Float(_)));
        assert!(matches!(Error::from(logging_error()), Error::Logging(_)));
        assert!(matches!(Error::from(clap_error()), Error::Cli(_)));
        assert!(matches!(Error::from(sqlx::Error::RowNotFound), Error::Database(_)));
        assert!(matches!(
            Error::from(sqlx::migrate::MigrateError::VersionMissing(1)),
            Error::Migrate(_)
        ));
    }
}
