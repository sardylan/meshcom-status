mod parse;

use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::errors::{Error, Result};
use crate::frame::{Frame, Observation};

const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
const META_WAIT: Duration = Duration::from_millis(500);

async fn send_all(out: &mpsc::Sender<Observation>, frames: Vec<Frame>) -> bool {
    for f in frames {
        if out.send(Observation::Frame(f)).await.is_err() {
            return false;
        }
    }
    true
}

async fn read_kiss(address: &str, out: &mpsc::Sender<Observation>) -> Result<()> {
    let mut stream = TcpStream::connect(address).await.map_err(|source| Error::Connect {
        what: "KISS port",
        address: address.to_owned(),
        source,
    })?;
    tracing::info!("KISS connected to {address}");
    let mut deframer = parse::Deframer::default();
    let mut assembler = parse::Assembler::default();
    let mut buf = [0u8; 2048];
    loop {
        let read = tokio::time::timeout(META_WAIT, stream.read(&mut buf)).await;
        let frames = match read {
            Err(_) => assembler.flush().into_iter().collect(),
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(n)) => deframer
                .feed(&buf[..n])
                .iter()
                .filter_map(|raw| {
                    parse::kiss(raw)
                        .map_err(|e| tracing::debug!("KISS: frame skipped: {e}"))
                        .ok()
                })
                .flat_map(|k| assembler.push(k))
                .collect(),
            Ok(Err(e)) => return Err(e.into()),
        };
        if !send_all(out, frames).await {
            return Ok(());
        }
    }
}

pub(crate) async fn run(host: String, port: u16, out: mpsc::Sender<Observation>) {
    let address = format!("{host}:{port}");
    let mut delay = RETRY_MIN;
    while !out.is_closed() {
        match read_kiss(&address, &out).await {
            Ok(()) => {
                tracing::warn!("KISS port at {address} closed the connection");
                delay = RETRY_MIN;
            }
            Err(e) => tracing::warn!("KISS: {e}; retrying in {} s", delay.as_secs()),
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RETRY_MAX);
    }
}
