use crate::{command::Command, session::Session};
use log::info;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use tokio::sync::mpsc;

pub(crate) struct IO {
    rx: mpsc::UnboundedReceiver<Command>,
    sessions: HashMap<String, Rc<RefCell<Session>>>,
}

impl IO {
    pub(crate) fn new(rx: mpsc::UnboundedReceiver<Command>) -> Self {
        Self {
            rx,
            sessions: HashMap::new(),
        }
    }

    pub(crate) async fn run(&mut self) {
        info!("IO::run starts");
        loop {
            match self.rx.recv().await {
                Some(command) => match command {
                    Command::CreateStream { target, tx } => {
                        if let Some(session) = self.sessions.get(&target) {
                            let session = Rc::clone(session);
                            tokio::task::spawn_local(async move {
                                let res = session
                                    .borrow_mut()
                                    .create_stream(
                                        1,
                                        Duration::from_secs(3),
                                        Duration::from_secs(1),
                                    )
                                    .await
                                    .map(|mut v| v.pop());
                                let _ = tx.send(res);
                            });
                        }
                    }
                },
                None => {
                    info!("Task channel is closed");
                    // log end of loop
                    break;
                }
            }
        }
        info!("IO::run completes");
    }
}
