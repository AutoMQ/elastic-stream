use std::{sync::Mutex, thread::JoinHandle};

use log::info;

#[derive(Debug)]
pub struct HandleJoiner {
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl HandleJoiner {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&mut self, handle: JoinHandle<()>) {
        if let Ok(mut handles) = self.handles.lock() {
            handles.push(handle);
        }
    }
}

impl Drop for HandleJoiner {
    fn drop(&mut self) {
        if let Ok(mut handles) = self.handles.lock() {
            while let Some(handle) = handles.pop() {
                info!("Joining a thread handle");
                let _ = handle.join();
            }
        }
    }
}
