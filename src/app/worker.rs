use std::sync::{LazyLock, Mutex};

use iced::{Subscription, futures::{SinkExt, Stream, StreamExt, channel::mpsc}, stream};

use super::Message;

const BUFFER_LENGTH: usize = 128;

static SENDER:
    LazyLock<Mutex<Option<mpsc::Sender<Message>>>>
    = LazyLock::new(Default::default);

fn stream() -> impl Stream<Item = Message> {
    stream::channel(BUFFER_LENGTH, async |mut output| {
        let (tx, mut rx) = mpsc::channel::<Message>(BUFFER_LENGTH);
        if let Some(mut sender) = SENDER.lock().ok() {
            *sender = Some(tx);
        }
        while let Some(message) = rx.next().await {
            let _ = output.send(message).await;
        }
    })
}

pub fn subscription() -> Subscription<Message> {
    Subscription::run(stream)
}

pub fn emit(message: Message) {
    if let Some(mut sender) = SENDER
        .lock().ok()
        .and_then(|r| r.as_ref().cloned())
    {
        let _ = sender.try_send(message);
    }
}
