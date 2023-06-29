#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum ClientRole {
    Unspecified,
    PlacementDriver,
    RangeServer,
    Frontend,
}

impl From<&ClientRole> for protocol::rpc::header::ClientRole {
    fn from(role: &ClientRole) -> Self {
        match role {
            ClientRole::Unspecified => protocol::rpc::header::ClientRole::CLIENT_ROLE_UNSPECIFIED,
            ClientRole::PlacementDriver => protocol::rpc::header::ClientRole::CLIENT_ROLE_PD,
            ClientRole::RangeServer => protocol::rpc::header::ClientRole::CLIENT_ROLE_RANGE_SERVER,
            ClientRole::Frontend => protocol::rpc::header::ClientRole::CLIENT_ROLE_FRONTEND,
        }
    }
}
