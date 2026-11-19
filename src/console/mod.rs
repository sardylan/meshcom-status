mod parse;

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::errors::{Error, Result};
use crate::frame::Observation;

const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

async fn read_console(address: &str, out: &mpsc::Sender<Observation>) -> Result<()> {
    let stream = TcpStream::connect(address).await.map_err(|source| Error::Connect {
        what: "net console",
        address: address.to_owned(),
        source,
    })?;
    tracing::info!("net console connected to {address}");
    let mut reader = BufReader::new(stream);
    let mut raw = Vec::with_capacity(512);
    loop {
        raw.clear();
        let n = tokio::time::timeout(IDLE_TIMEOUT, reader.read_until(b'\n', &mut raw))
            .await
            .map_err(|_| Error::Invalid {
                what: "net console",
                input: address.to_owned(),
                reason: "no data for 120 s".into(),
            })??;
        if n == 0 {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&raw);
        match parse::line(&text) {
            Ok(parse::Line::RxFrame { signal, bytes }) => match parse::decode(&bytes, signal) {
                Ok(Some(frame)) => {
                    if out.send(Observation::Frame(frame)).await.is_err() {
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::debug!("net console: frame skipped: {e}"),
            },
            Ok(parse::Line::Dropped(n)) => tracing::warn!("net console: the node dropped {n} captured frames"),
            Ok(parse::Line::AuthRequired) => {
                return Err(Error::Invalid {
                    what: "net console",
                    input: address.to_owned(),
                    reason: "the node asks for a password (--passwd), which is not supported yet".into(),
                });
            }
            Ok(parse::Line::Other) => {}
            Err(e) => tracing::debug!("net console: line skipped: {e}"),
        }
    }
}

pub(crate) async fn run(host: String, port: u16, out: mpsc::Sender<Observation>) {
    let address = format!("{host}:{port}");
    let mut delay = RETRY_MIN;
    while !out.is_closed() {
        match read_console(&address, &out).await {
            Ok(()) => {
                tracing::warn!("net console at {address} closed the connection");
                delay = RETRY_MIN;
            }
            Err(e) => {
                tracing::warn!("net console: {e}; retrying in {} s", delay.as_secs());
            }
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RETRY_MAX);
    }
}
