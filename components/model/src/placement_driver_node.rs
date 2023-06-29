use protocol::rpc::header::PlacementDriverNodeT;

/// Represent a placement driver member.
///
/// # Note
/// Leadership of the node may change. On receiving error code from a node, claiming it is not
/// leader, the cached metadata should be updated accordingly to reflect actual cluster status quo.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementDriverNode {
    pub name: String,
    pub advertise_addr: String,
    pub leader: bool,
}

impl From<&PlacementDriverNodeT> for PlacementDriverNode {
    fn from(value: &PlacementDriverNodeT) -> Self {
        Self {
            name: value.name.clone(),
            advertise_addr: value.advertise_addr.clone(),
            leader: value.is_leader,
        }
    }
}
