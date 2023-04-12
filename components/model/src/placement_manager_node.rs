/// Represent a placement manager member.
///
/// # Note
/// Leadership of the node may change. On receiving error code from a node, claiming it is not
/// leader, the cached metadata should be updated accordingly to reflect actual cluster status quo.
pub struct PlacementManagerNode {
    pub name: String,
    pub advertise_addr: String,
    pub leader: bool,
}
