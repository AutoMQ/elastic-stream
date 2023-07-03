use bytes::Bytes;
use bytes::BytesMut;
use log::debug;
use log::warn;
use opendal::Operator;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use std::vec::Vec;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::ObjectManager;
use crate::ObjectMetadata;

pub struct RangeOffload<M: ObjectManager + 'static> {
    stream_id: u64,
    range_index: u32,
    object_size: u32,
    multi_part_object: RefCell<Option<MultiPartObject>>,
    op: Operator,
    object_manager: Rc<M>,
}

impl<M> RangeOffload<M>
where
    M: ObjectManager + 'static,
{
    pub fn new(
        stream_id: u64,
        range_index: u32,
        op: Operator,
        object_manager: Rc<M>,
        object_size: u32,
    ) -> RangeOffload<M> {
        Self {
            stream_id,
            range_index,
            object_size,
            multi_part_object: RefCell::new(None),
            op,
            object_manager,
        }
    }

    pub fn write(&self, start_offset: u64, end_offset: u64, payload: Vec<Bytes>) {
        // TODO: 统计 inflight bytes 来进行背压，避免网络打爆。或者通过一个全局的并发限制器。如果限制则直接返回 false，或者卡住。
        let payload_length: usize = payload.iter().map(|p| p.len()).sum();
        let payload_length = payload_length as u32;
        // TODO: key name, reverse hex representation of start_offset as prefix.
        let key = format!(
            "ess3test/{}-{}/{}",
            self.stream_id, self.range_index, start_offset
        );

        let mut multi_part_object = self.multi_part_object.borrow_mut();
        if let Some(multi_part_obj) = multi_part_object.as_ref() {
            // the last multi-part object exist, then write to it.
            if multi_part_obj.write(payload, end_offset) {
                multi_part_obj.close();
                *multi_part_object = None;
            }
            return;
        }

        if payload_length >= self.object_size {
            let mut object_metadata =
                ObjectMetadata::new(self.stream_id, self.range_index, start_offset);
            object_metadata.end_offset_delta = (end_offset - start_offset) as u32;
            // direct write when payload length is larger than object_size.
            let object = Object {
                key,
                op: self.op.clone(),
                object_manager: self.object_manager.clone(),
            };
            object.write(payload, object_metadata);
            return;
        }

        // start a new multi-part object.
        let object_metadata = ObjectMetadata::new(self.stream_id, self.range_index, start_offset);
        let new_multi_part_object = MultiPartObject::new(
            object_metadata,
            key,
            self.op.clone(),
            self.object_manager.clone(),
            self.object_size,
        );
        new_multi_part_object.write(payload, end_offset);
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
    tx: mpsc::UnboundedSender<(Vec<Bytes>, u64)>,
}

impl MultiPartObject {
    pub fn new<M: ObjectManager + 'static>(
        object_metadata: ObjectMetadata,
        key: String,
        op: Operator,
        object_manager: Rc<M>,
        object_size: u32,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<(Vec<Bytes>, u64)>();
        {
            let key = key;
            let op = op;
            Self::write_loop(object_metadata, key, op, object_manager, rx);
        }
        Self {
            object_size,
            size: RefCell::new(0),
            tx,
        }
    }

    pub fn write(&self, part: Vec<Bytes>, end_offset: u64) -> bool {
        let mut size = self.size.borrow_mut();
        let part_length: usize = part.iter().map(|p| p.len()).sum();
        *size += part_length as u32;
        let _ = self.tx.send((part, end_offset));
        *size >= self.object_size
    }

    pub fn close(&self) {
        let _ = self.tx.send((Vec::new(), 0));
    }

    fn write_loop<M: ObjectManager + 'static>(
        mut object_metadata: ObjectMetadata,
        key: String,
        op: Operator,
        object_manager: Rc<M>,
        mut rx: mpsc::UnboundedReceiver<(Vec<Bytes>, u64)>,
    ) {
        tokio_uring::spawn(async move {
            // TODO: delay init multi-part object, if there is only one part or several parts is too small. the multi-part object is not necessary.
            // use single Object put instead to save API call.
            let mut writer = None;
            loop {
                match rx.recv().await {
                    Some((part, end_offset)) => {
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
                                // TODO: footer & sparse index
                                let rst = writer.close().await;
                                object_metadata.end_offset_delta =
                                    (end_offset - object_metadata.start_offset) as u32;
                                object_manager.commit_object(object_metadata.clone());
                                rst
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

struct Object<M: ObjectManager + 'static> {
    key: String,
    op: Operator,
    object_manager: Rc<M>,
}

impl<M> Object<M>
where
    M: ObjectManager + 'static,
{
    pub fn write(&self, payload: Vec<Bytes>, object_metadata: ObjectMetadata) {
        let key = self.key.clone();
        let op = self.op.clone();
        let object_manager = self.object_manager.clone();
        tokio_uring::spawn(async move {
            let payload_length: usize = payload.iter().map(|p| p.len()).sum();
            let mut bytes = BytesMut::with_capacity(payload_length);
            for b in payload.iter() {
                bytes.extend_from_slice(b);
            }
            // TODO: footer and sparse index
            loop {
                debug!("{key} start write object[len={payload_length}]");
                match op.write(&key, bytes.clone()).await {
                    Ok(_) => {
                        debug!("{key} write object success[len={payload_length}]");
                        object_manager.commit_object(object_metadata);
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
