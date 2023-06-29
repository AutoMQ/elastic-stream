use bytes::Bytes;
use bytes::BytesMut;
use log::debug;
use log::warn;
use opendal::Operator;
use std::cell::RefCell;
use std::time::Duration;
use std::vec::Vec;
use tokio::sync::mpsc;
use tokio::time::sleep;

pub struct RangeOffload {
    object_size: u32,
    multi_part_object: RefCell<Option<MultiPartObject>>,
    op: Operator,
}

impl RangeOffload {
    pub fn new(op: Operator, object_size: u32) -> Self {
        RangeOffload {
            object_size,
            multi_part_object: RefCell::new(None),
            op,
        }
    }

    pub fn write(&self, start_offset: u64, _end_offset: u64, payload: Vec<Bytes>) {
        // TODO: 统计 inflight bytes 来进行背压，避免网络打爆。或者通过一个全局的并发限制器。如果限制则直接返回 false，或者卡住。
        let payload_length: usize = payload.iter().map(|p| p.len()).sum();
        let payload_length = payload_length as u32;
        let key = format!("cluster/{}", start_offset);

        let mut multi_part_object = self.multi_part_object.borrow_mut();
        if let Some(multi_part_obj) = multi_part_object.as_ref() {
            // the last multi-part object exist, then write to it.
            if multi_part_obj.write(payload) {
                multi_part_obj.close();
                *multi_part_object = None;
            }
            return;
        }

        if payload_length >= self.object_size {
            // direct write when payload length is larger than object_size.
            let object = Object {
                key,
                op: self.op.clone(),
            };
            object.write(payload);
            return;
        }

        // start a new multi-part object.
        let new_multi_part_object = MultiPartObject::new(key, self.op.clone(), self.object_size);
        new_multi_part_object.write(payload);
        *multi_part_object = Some(new_multi_part_object);
    }

    /// force inflight multi-part object to complete.
    pub fn flush(&self) {
        todo!("flush multi-part object")
    }
}

struct MultiPartObject {
    object_size: u32,
    size: RefCell<u32>,
    tx: mpsc::UnboundedSender<Vec<Bytes>>,
}

impl MultiPartObject {
    pub fn new(key: String, op: Operator, object_size: u32) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<Vec<Bytes>>();
        {
            let key = key;
            let op = op;
            Self::write_loop(key, op, rx);
        }
        Self {
            object_size,
            size: RefCell::new(0),
            tx,
        }
    }

    pub fn write(&self, part: Vec<Bytes>) -> bool {
        let mut size = self.size.borrow_mut();
        let part_length: usize = part.iter().map(|p| p.len()).sum();
        *size += part_length as u32;
        let _ = self.tx.send(part);
        *size >= self.object_size
    }

    pub fn close(&self) {
        let _ = self.tx.send(Vec::new());
    }

    fn write_loop(key: String, op: Operator, mut rx: mpsc::UnboundedReceiver<Vec<Bytes>>) {
        tokio_uring::spawn(async move {
            // TODO: delay init multi-part object, if there is only one part or several parts is too small. the multi-part object is not necessary.
            // use single Object put instead to save API call.
            let mut writer = None;
            loop {
                match rx.recv().await {
                    Some(part) => {
                        // get multi-part object writer.
                        if writer.is_none() {
                            loop {
                                writer = match op.writer(&key).await {
                                    Ok(w) => Some(w),
                                    Err(e) => {
                                        warn!("{key} get writer fail, retry later, {e}");
                                        sleep(Duration::from_secs(1)).await;
                                        continue;
                                    }
                                };
                                break;
                            }
                        }
                        let writer = writer.as_mut().unwrap();

                        // write part or complete object.
                        let payload_length: usize = part.iter().map(|p| p.len()).sum();
                        let mut bytes = BytesMut::with_capacity(payload_length);
                        for b in part.iter() {
                            bytes.extend_from_slice(b);
                        }
                        let bytes = bytes.freeze();
                        let close_part = bytes.is_empty();
                        let mut closed = false;
                        loop {
                            let result = if close_part {
                                // TODO: footer
                                writer.close().await
                            } else {
                                writer.write(bytes.clone()).await
                            };
                            match result {
                                Ok(_) => {
                                    if close_part {
                                        closed = true;
                                        debug!("{key} close multi-part object success");
                                    } else {
                                        debug!("{key} write multi-part object part[len={payload_length}] success");
                                    }
                                    break;
                                }
                                Err(e) => {
                                    warn!("{key} write part or close fail, retry later, {e}");
                                    sleep(Duration::from_secs(1)).await;
                                    continue;
                                }
                            }
                        }
                        if closed {
                            break;
                        }
                    }
                    None => {
                        todo!("handle rx close")
                    }
                }
            }
        });
    }
}

struct Object {
    key: String,
    op: Operator,
}

impl Object {
    pub fn write(&self, payload: Vec<Bytes>) {
        let key = self.key.clone();
        let op = self.op.clone();
        tokio_uring::spawn(async move {
            let payload_length: usize = payload.iter().map(|p| p.len()).sum();
            let mut bytes = BytesMut::with_capacity(payload_length);
            for b in payload.iter() {
                bytes.extend_from_slice(b);
            }
            // TODO: footer
            loop {
                debug!("{key} start write object[len={payload_length}]");
                match op.write(&key, bytes.clone()).await {
                    Ok(_) => {
                        debug!("{key} write object success[len={payload_length}]");
                        break;
                    }
                    Err(e) => {
                        // retry later until success.
                        warn!("{key} write object, retry later, {e}");
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
        });
    }
}
