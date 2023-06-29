use protocol::rpc::header::RangeServerT;

#[derive(Debug, Clone)]
pub struct DataNode {
    pub node_id: i32,
    pub advertise_address: String,
}

impl DataNode {
    pub fn new<Addr>(id: i32, address: Addr) -> Self
    where
        Addr: AsRef<str>,
    {
        Self {
            node_id: id,
            advertise_address: address.as_ref().to_owned(),
        }
    }
}

impl From<&DataNode> for RangeServerT {
    fn from(value: &DataNode) -> Self {
        let mut ret = RangeServerT::default();
        ret.server_id = value.node_id;
        ret.advertise_addr = value.advertise_address.clone();
        ret
    }
}

impl From<&RangeServerT> for DataNode {
    fn from(value: &RangeServerT) -> Self {
        Self {
            node_id: value.server_id,
            advertise_address: value.advertise_addr.clone(),
        }
    }
}
