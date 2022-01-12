use std::sync::Arc;

use async_stream::stream;
use futures::{
    channel::mpsc::{channel, Receiver},
    pin_mut, SinkExt, Stream, StreamExt,
};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use value::{de::DeserializerError, Value};

use crate::{Config, ConfigFinder, Error};

fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(1);

    let watcher = RecommendedWatcher::new(move |res| {
        futures::executor::block_on(async {
            tx.send(res).await.unwrap();
        })
    })?;

    Ok((watcher, rx))
}

pub(crate) fn watch<B: Backend + 'static>(
    finder: ConfigFinder<B>,
) -> impl Stream<Item = Result<Config, Error>> + Send {
    let stream = stream! {
        let (mut watcher, mut recv) = async_watcher().unwrap();

        let roots = finder.0.locators.iter().map(|l| l.root());

        for root in roots {
            watcher.watch(root, RecursiveMode::NonRecursive).unwrap();
        }

        let mut last: Option<Event> = None;
        let mut last_time = std::time::Instant::now();

        while let Some(event) = recv.next().await {
            let event = match event {
                Ok(event) => event,
                Err(err) => {
                    println!("error: {:?}",err);
                    continue;
                }
            };

            if let Some(l) = &last {
                let diff = std::time::Instant::now().duration_since(last_time);
                if l == &event && diff < std::time::Duration::from_millis(500) {
                    continue;
                }
            }

            last = Some(event.clone());
            last_time = std::time::Instant::now();

            let paths = match &event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    &event.paths
                }
                _ => continue,
            };
            if finder.matche_any(&paths) {
                let cfg = finder.config().await;
                yield cfg;
            }
        }
    };

    stream
}

use async_broadcast::{broadcast, Receiver as BroadcastReceiver, Sender};
use async_lock::RwLock;
use brunson::{Backend, Runtime};
use futures::channel::oneshot::{channel as oneshot, Sender as KillSender};

// #[derive(Clone)]
pub struct WatchableConfig<B: Backend> {
    config: Arc<RwLock<Config>>,
    finder: ConfigFinder<B>,
    broadcast: BroadcastReceiver<()>,
    kill: Option<KillSender<()>>,
}

impl<B: Backend + 'static> WatchableConfig<B> {
    pub async fn new<R: Runtime>(runtime: R, finder: ConfigFinder<B>) -> WatchableConfig<B> {
        let (sx, rx) = broadcast(10);
        let (killsx, mut killrx) = oneshot();

        let cfg = finder.config().await.unwrap_or_default();

        let cfg = WatchableConfig {
            config: Arc::new(RwLock::new(cfg)),
            finder: finder.clone(),
            broadcast: rx,
            kill: Some(killsx),
        };

        let config = cfg.config.clone();

        runtime.spawn(async move {
            let watcher = finder.watch().fuse();
            pin_mut!(watcher);

            loop {
                let item = futures::select! {
                    item = watcher.next() => {
                        match item {
                            Some(item) => item,
                            None => continue
                        }
                    },
                    _ = killrx => {
                        break
                    }
                };

                if let Ok(cfg) = item {
                    *config.write().await = cfg;
                    if sx.broadcast(()).await.is_err() {
                        break;
                    }
                }
            }
        });

        cfg
    }

    pub async fn get(&self, name: impl AsRef<str>) -> Option<Value> {
        self.config.read().await.get(name).cloned()
    }

    pub async fn try_get<'a, S: serde::Deserialize<'a>>(
        &self,
        name: &str,
    ) -> Result<S, DeserializerError> {
        let cfg = self.config.read().await;
        cfg.try_get(name)
    }

    pub async fn set(&self, name: impl ToString, value: impl Into<Value>) -> Option<Value> {
        let mut cfg = self.config.write().await;
        cfg.set(name.to_string(), value.into())
    }

    pub async fn contains(&self, name: impl AsRef<str>) -> bool {
        let cfg = self.config.read().await;
        cfg.contains(name)
    }

    pub async fn snapshot(&self) -> Config {
        let cfg = self.config.read().await;
        cfg.clone()
    }

    pub fn listen(&self) -> impl Stream<Item = ()> + Send {
        self.broadcast.clone()
    }
}

impl<B: Backend> Drop for WatchableConfig<B> {
    fn drop(&mut self) {
        self.kill.take().unwrap().send(()).ok();
    }
}
