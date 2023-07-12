use std::{cell::RefCell, collections::HashMap};

use crate::{ObjectManager, Owner, RangeKey};
use model::object::ObjectMetadata;

pub struct MemoryObjectManager {
    map: RefCell<HashMap<RangeKey, Vec<ObjectMetadata>>>,
}

impl ObjectManager for MemoryObjectManager {
    fn is_owner(&self, _stream_id: u64, _range_index: u32) -> Option<Owner> {
        Some(Owner {
            epoch: 0,
            start_offset: 0,
        })
    }

    fn commit_object(&self, object_metadata: ObjectMetadata) {
        let key = RangeKey::new(object_metadata.stream_id, object_metadata.range_index);
        let mut map = self.map.borrow_mut();
        let metas = if let Some(metas) = map.get_mut(&key) {
            metas
        } else {
            let metas = vec![];
            map.insert(key, metas);
            map.get_mut(&key).unwrap()
        };
        metas.push(object_metadata);
    }

    fn get_objects(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
        _size_hint: u32,
    ) -> Vec<ObjectMetadata> {
        let key = RangeKey::new(stream_id, range_index);
        if let Some(metas) = self.map.borrow().get(&key) {
            metas
                .iter()
                .filter(|meta| {
                    meta.start_offset < end_offset
                        && (meta.end_offset_delta as u64 + meta.start_offset) >= start_offset
                })
                .map(|meta| {
                    let mut meta = meta.clone();
                    let key = format!(
                        "ess3test/{}-{}/{}",
                        stream_id, range_index, meta.start_offset
                    );
                    meta.key = Some(key);
                    meta
                })
                .collect()
        } else {
            vec![]
        }
    }
}

impl Default for MemoryObjectManager {
    fn default() -> Self {
        Self {
            map: RefCell::new(HashMap::new()),
        }
    }
}
