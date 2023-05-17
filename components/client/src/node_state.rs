/// Represents the state of a placement manager node in the Raft protocol.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum NodeState {
    Unknown,
    Leader,
    Follower,
    Candidate,
}
