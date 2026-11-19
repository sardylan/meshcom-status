use std::time::SystemTime;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

use crate::clock;
use crate::config::LogLevel;
use crate::errors::Result;

pub(crate) const ACCESS: &str = "access";

struct UtcMicros;

impl FormatTime for UtcMicros {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", clock::rfc3339_micros(clock::unix_micros(SystemTime::now())))
    }
}

fn filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Off => LevelFilter::OFF,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Warning => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Trace => LevelFilter::TRACE,
    }
}

pub(crate) fn start(level: LogLevel) -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_timer(UtcMicros)
        .with_max_level(filter(level))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_one_to_one() {
        assert_eq!(filter(LogLevel::Off), LevelFilter::OFF);
        assert_eq!(filter(LogLevel::Error), LevelFilter::ERROR);
        assert_eq!(filter(LogLevel::Warning), LevelFilter::WARN);
        assert_eq!(filter(LogLevel::Info), LevelFilter::INFO);
        assert_eq!(filter(LogLevel::Debug), LevelFilter::DEBUG);
        assert_eq!(filter(LogLevel::Trace), LevelFilter::TRACE);
    }

    #[test]
    fn timestamp_shape() {
        let mut s = String::new();
        UtcMicros.format_time(&mut Writer::new(&mut s)).unwrap();
        assert_eq!(s.len(), "2026-11-13T10:15:03.123456Z".len(), "{s}");
        assert!(s.ends_with('Z'));
    }
}
