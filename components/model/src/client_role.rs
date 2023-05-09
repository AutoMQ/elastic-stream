#[derive(Debug, Clone)]
pub enum ClientRole {
    Unspecified,
    PlacementManager,
    DataNode,
}

impl From<&ClientRole> for protocol::rpc::header::ClientRole {
    fn from(role: &ClientRole) -> Self {
        match role {
            ClientRole::Unspecified => protocol::rpc::header::ClientRole::CLIENT_ROLE_UNSPECIFIED,
            ClientRole::PlacementManager => protocol::rpc::header::ClientRole::CLIENT_ROLE_PM,
            ClientRole::DataNode => protocol::rpc::header::ClientRole::CLIENT_ROLE_DATA_NODE,
        }
    }
}
