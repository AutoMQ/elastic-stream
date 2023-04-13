use protocol::rpc::header::PlacementManagerNodeT;

/// Represent a placement manager member.
///
/// # Note
/// Leadership of the node may change. On receiving error code from a node, claiming it is not
/// leader, the cached metadata should be updated accordingly to reflect actual cluster status quo.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementManagerNode {
    pub name: String,
    pub advertise_addr: String,
    pub leader: bool,
}

impl From<&PlacementManagerNodeT> for PlacementManagerNode {
    fn from(value: &PlacementManagerNodeT) -> Self {
        Self {
            name: value.name.clone(),
            advertise_addr: value.advertise_addr.clone(),
            leader: value.is_leader,
        }
    }
}
