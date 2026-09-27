//! Background work that reports back to the event loop.
//!
//! A task gets a [`Mailbox`] addressed to the screen that started it and sends
//! [`Msg`]s through it (progress, then a result). The [`Task`] handle aborts
//! the work when dropped, so a screen that closes takes its work with it, and
//! results for screens that are gone are dropped by the app.

use std::future::Future;

use tokio::sync::mpsc::UnboundedSender;

use crate::msg::Msg;
use crate::screen::ScreenId;

/// Everything the event loop reacts to.
#[derive(Debug)]
pub enum Event {
    Term(crossterm::event::Event),
    Msg(ScreenId, Msg),
}

/// Sends messages from a task to the screen that started it.
#[derive(Debug, Clone)]
pub struct Mailbox {
    tx: UnboundedSender<Event>,
    to: ScreenId,
}

impl Mailbox {
    /// Delivers `msg`. Silently dropped if the app has shut down.
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(Event::Msg(self.to, msg));
    }
}

/// Running background work; aborted when dropped.
#[derive(Debug)]
pub struct Task(tokio::task::JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Starts background work for one screen.
#[derive(Debug, Clone)]
pub struct Tasks {
    tx: UnboundedSender<Event>,
    runtime: tokio::runtime::Handle,
    owner: ScreenId,
}

impl Tasks {
    pub fn new(tx: UnboundedSender<Event>, runtime: tokio::runtime::Handle) -> Self {
        Self {
            tx,
            runtime,
            owner: 0,
        }
    }

    /// The same spawner, reporting to `owner`.
    pub fn for_screen(&self, owner: ScreenId) -> Self {
        Self {
            owner,
            ..self.clone()
        }
    }

    /// A mailbox addressed to this spawner's screen.
    pub fn mailbox(&self) -> Mailbox {
        Mailbox {
            tx: self.tx.clone(),
            to: self.owner,
        }
    }

    /// Runs `work` in the background, giving it a mailbox to report through.
    pub fn spawn<F, Fut>(&self, work: F) -> Task
    where
        F: FnOnce(Mailbox) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Task(self.runtime.spawn(work(self.mailbox())))
    }

    /// Runs `work` to its end even if its screen closes first: for work that
    /// must land, like saving, and that finishes by itself.
    pub fn spawn_detached<F, Fut>(&self, work: F)
    where
        F: FnOnce(Mailbox) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        drop(self.runtime.spawn(work(self.mailbox())));
    }

    /// Runs `work` in the background and delivers its result as one message.
    pub fn run<T, Fut>(&self, work: Fut, into_msg: impl FnOnce(T) -> Msg + Send + 'static) -> Task
    where
        T: Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        self.spawn(|mail| async move { mail.send(into_msg(work.await)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn results_reach_the_owner_and_dropping_aborts() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tasks = Tasks::new(tx, tokio::runtime::Handle::current()).for_screen(7);
        let _task = tasks.run(async { 41 + 1 }, Msg::Test);
        match rx.recv().await {
            Some(Event::Msg(7, Msg::Test(42))) => {}
            other => panic!("unexpected {other:?}"),
        }

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task = tasks.spawn(|mail| async move {
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
            mail.send(Msg::Test(0));
        });
        started_rx.await.unwrap();
        drop(task);
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "an aborted task sends nothing");
    }
}
