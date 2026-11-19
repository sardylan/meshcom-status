use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::SystemTime;

use tokio::sync::{broadcast, mpsc, watch};

use uuid::Uuid;

use crate::chat::{ChatMessage, Target, TargetInfo};
use crate::config::Config;
use crate::errors::{Error, Result};
use crate::frame::{Call, Observation, Reception};
use crate::graph::{Node, Nodes};
use crate::stats::{Range, Stats};
use crate::store::Store;
use crate::view::View;
use crate::{clock, console, import, kiss, node_http, web};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event {
    Node(Node),
    Reception(Reception),
}

pub(crate) struct Hub {
    nodes: RwLock<Nodes>,
    groups: RwLock<Vec<String>>,
    store: Store,
    events: broadcast::Sender<Event>,
}

impl Hub {
    fn new(store: Store, nodes: Nodes) -> Hub {
        Hub {
            nodes: RwLock::new(nodes),
            groups: RwLock::new(vec![]),
            store,
            events: broadcast::channel(1024).0,
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub(crate) fn nodes(&self) -> Vec<Node> {
        // A panic while holding the lock cannot leave the registry half-updated in a way that
        // matters to readers, so a poisoned lock is still used.
        self.nodes.read().unwrap_or_else(PoisonError::into_inner).all()
    }

    pub(crate) async fn receptions(&self, view: &View) -> Result<Vec<Reception>> {
        self.store.receptions(view).await
    }

    pub(crate) fn own_call(&self) -> Option<String> {
        self.nodes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .own()
            .map(str::to_owned)
    }

    pub(crate) fn chat_tabs(&self) -> Vec<Target> {
        let groups = self.groups.read().unwrap_or_else(PoisonError::into_inner).clone();
        std::iter::once(Target::Broadcast)
            .chain(groups.into_iter().map(Target::Channel))
            .collect()
    }

    pub(crate) fn node_groups(&self) -> Vec<String> {
        self.groups.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub(crate) async fn chat_page(
        &self,
        target: &Target,
        before: Option<Uuid>,
        limit: u32,
    ) -> Result<(Vec<ChatMessage>, bool)> {
        self.store.chat_page(target, before, limit).await
    }

    pub(crate) async fn chat_message_of(&self, r: &Reception) -> Result<Option<ChatMessage>> {
        self.store.chat_message_of(r).await
    }

    pub(crate) async fn chat_targets(&self, prefix: &str, limit: u32, exclude: &[Target]) -> Result<Vec<TargetInfo>> {
        self.store.chat_targets(prefix, limit, exclude).await
    }

    pub(crate) async fn stats(&self, range: Range, tz: &str) -> Result<Stats> {
        self.store.stats(range, clock::unix_micros(SystemTime::now()), tz).await
    }

    fn publish(&self, events: Vec<Event>) {
        events.into_iter().for_each(|e| {
            let _ = self.events.send(e);
        });
    }

    async fn apply(&self, observation: Observation, now: u64) -> Result<()> {
        match observation {
            Observation::Frame(frame) => {
                let path = self
                    .nodes
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .path_of(&frame);
                let reception = self.store.insert_reception(now, &path, &frame).await?;
                let changed = self
                    .nodes
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply(&reception);
                self.store.save_nodes(&changed).await?;
                self.publish(
                    changed
                        .into_iter()
                        .map(Event::Node)
                        .chain([Event::Reception(reception)])
                        .collect(),
                );
            }
            Observation::Own(own) => {
                *self.groups.write().unwrap_or_else(PoisonError::into_inner) = own.groups.clone();
                let changed = self
                    .nodes
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply_own(own, now);
                self.store.save_nodes(&changed).await?;
                self.publish(changed.into_iter().map(Event::Node).collect());
            }
        }
        Ok(())
    }
}

fn now() -> u64 {
    clock::unix_micros(SystemTime::now())
}

async fn apply_observations(hub: Arc<Hub>, mut rx: mpsc::Receiver<Observation>) {
    while let Some(observation) = rx.recv().await {
        if let Err(e) = hub.apply(observation, now()).await {
            tracing::error!("observation lost: {e}");
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::warn!("cannot watch SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        r = ctrl_c => if let Err(e) = r { tracing::warn!("cannot watch Ctrl-C: {e}") },
        () = term => {}
    }
    tracing::info!("shutting down");
}

fn start_sources(config: &Config, tx: &mpsc::Sender<Observation>) {
    let node = &config.node;
    let Some(host) = node.host.clone() else {
        tracing::warn!("no node host configured (--node-host): no new frames will be received");
        return;
    };
    tokio::spawn(node_http::run(host.clone(), node.http_port, tx.clone()));
    if node.console_enabled {
        tokio::spawn(console::run(host.clone(), node.console_port, tx.clone()));
    }
    if node.kiss_enabled {
        tokio::spawn(kiss::run(host, node.kiss_port, tx.clone()));
    }
    if !node.console_enabled && !node.kiss_enabled {
        tracing::warn!("both the net console and KISS are disabled: no frames will be received");
    }
}

pub(crate) async fn run(config: Config) -> Result<()> {
    let store = Store::connect(&config.database.url).await?;
    let stored = store.load_nodes().await?;
    tracing::info!("{} nodes loaded from the database", stored.len());
    let hub = Arc::new(Hub::new(store, Nodes::from_stored(stored)));

    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(apply_observations(hub.clone(), rx));
    start_sources(&config, &tx);
    drop(tx);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    web::serve(&config.web, hub, shutdown_rx).await
}

pub(crate) async fn migrate(config: Config) -> Result<()> {
    Store::connect(&config.database.url).await?;
    tracing::info!("database migrations applied");
    Ok(())
}

pub(crate) async fn import(config: Config, path: &Path) -> Result<()> {
    let wrap = |source: Error| Error::Import {
        path: path.to_owned(),
        source: Box::new(source),
    };
    let text = tokio::fs::read_to_string(path).await.map_err(|e| wrap(e.into()))?;
    let imported = import::parse(&text).map_err(wrap)?;

    let store = Store::connect(&config.database.url).await?;
    let mut nodes = Nodes::from_stored(store.load_nodes().await?);
    for r in &imported.receptions {
        let stored = store.insert_reception(r.at, &r.path, &r.frame).await?;
        nodes.apply(&stored);
    }
    let current = nodes.all();
    let listed: BTreeSet<Call> = imported.nodes.iter().map(|n| n.call.clone()).collect();
    let merged: Vec<Node> = imported
        .nodes
        .into_iter()
        .map(|n| {
            let existing = current.iter().find(|c| c.call == n.call);
            import::merge_node(existing, n)
        })
        .chain(current.iter().filter(|c| !listed.contains(&c.call)).cloned())
        .collect();
    store.save_nodes(&merged).await?;
    tracing::info!(
        "imported {} receptions and {} nodes from {}",
        imported.receptions.len(),
        merged.len(),
        path.display()
    );
    Ok(())
}
