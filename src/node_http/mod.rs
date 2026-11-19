mod parse;

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::aprs::Position;
use crate::chat;
use crate::errors::{Error, Result};
use crate::frame::{self, Observation, OwnNode};

const TIMEOUT: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(30);
const REFRESH: Duration = Duration::from_secs(600);

async fn fetch(address: &str, host: &str, path: &str) -> Result<Vec<u8>> {
    let exchange = async {
        let mut stream = TcpStream::connect(address).await.map_err(|source| Error::Connect {
            what: "node HTTP",
            address: address.to_owned(),
            source,
        })?;
        let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nX-MC: 1\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await?;
        Ok(raw)
    };
    tokio::time::timeout(TIMEOUT, exchange)
        .await
        .map_err(|_| Error::Invalid {
            what: "node HTTP",
            input: format!("{address}{path}"),
            reason: "timed out".into(),
        })?
}

async fn getparam(address: &str, host: &str, name: &str) -> Result<String> {
    let raw = fetch(address, host, &format!("/getparam/?{name}")).await?;
    parse::getparam(&raw, name)
}

fn own_node(call: &str, lat: &str, lon: &str, alt: &str, setgrc: &str) -> Result<OwnNode> {
    let lat: f64 = lat.trim().parse()?;
    let lon: f64 = lon.trim().parse()?;
    let alt_m = alt.trim().parse::<f64>().ok();
    let position = (lat != 0.0 || lon != 0.0).then_some(Position { lat, lon, alt_m });
    Ok(OwnNode {
        call: frame::call(call)?,
        position,
        groups: chat::node_groups(setgrc),
    })
}

async fn read_own_node(address: &str, host: &str) -> Result<OwnNode> {
    let call = getparam(address, host, "setcall").await?;
    let lat = getparam(address, host, "setlat").await?;
    let lon = getparam(address, host, "setlon").await?;
    let alt = getparam(address, host, "setalt").await?;
    let groups = getparam(address, host, "setgrc").await?;
    own_node(&call, &lat, &lon, &alt, &groups)
}

pub(crate) async fn run(host: String, port: u16, out: mpsc::Sender<Observation>) {
    let address = format!("{host}:{port}");
    while !out.is_closed() {
        let delay = match read_own_node(&address, &host).await {
            Ok(own) => {
                tracing::info!("own node is {} at {:?}", own.call, own.position);
                if out.send(Observation::Own(own)).await.is_err() {
                    return;
                }
                REFRESH
            }
            Err(e) => {
                tracing::warn!("cannot read the own node: {e}; retrying in {} s", RETRY.as_secs());
                RETRY
            }
        };
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_node_from_parameters() {
        let own = own_node("qa0own-11", "10.234620", "-30.193300", "13", "123;12302;0;0;0;0;").unwrap();
        assert_eq!(own.call, "QA0OWN-11");
        assert_eq!(own.groups, vec!["123", "12302"]);
        assert_eq!(
            own.position,
            Some(Position {
                lat: 10.23462,
                lon: -30.1933,
                alt_m: Some(13.0)
            })
        );
    }

    #[test]
    fn unset_position_and_bad_values() {
        assert_eq!(own_node("QA0OWN-11", "0.0", "0.0", "", "").unwrap().position, None);
        assert!(own_node("QA0OWN-11", "x", "0", "0", "").is_err());
        assert!(own_node("", "1", "1", "0", "").is_err());
    }
}
