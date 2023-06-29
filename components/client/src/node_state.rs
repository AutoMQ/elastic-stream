/// Represents the state of a placement driver node in the Raft protocol.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum NodeState {
    Unknown,
    Leader,
    Follower,
}
