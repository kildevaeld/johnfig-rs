use async_stream::stream;
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, Stream, StreamExt,
};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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

pub(crate) fn watch(
    finder: ConfigFinder,
) -> Result<impl Stream<Item = Result<Config, Error>> + Send, Error> {
    let (mut watcher, mut recv) = async_watcher().unwrap();
    let stream = stream! {

        yield finder.config().await;

        let roots = finder.0.locators.iter().map(|l| l.root());

        for root in roots {
            watcher.watch(root, RecursiveMode::Recursive).unwrap();
        }

        while let Some(event) = recv.next().await {
            let event = match event {
                Ok(event) => event,
                Err(err) => {
                    println!("error: {:?}",err);
                    continue;
                }
            };

            let paths = match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    event.paths
                }
                _ => continue,
            };
            if finder.matche_any(&paths) {
                let cfg = finder.config().await;
                yield cfg;
            }
        }
    };

    Ok(stream)
}
