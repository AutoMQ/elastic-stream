use protocol::rpc::header::RangeServerT;

#[derive(Debug, Clone)]
pub struct RangeServer {
    pub server_id: i32,
    pub advertise_address: String,
}

impl RangeServer {
    pub fn new<Addr>(id: i32, address: Addr) -> Self
    where
        Addr: AsRef<str>,
    {
        Self {
            server_id: id,
            advertise_address: address.as_ref().to_owned(),
        }
    }
}

impl From<&RangeServer> for RangeServerT {
    fn from(value: &RangeServer) -> Self {
        let mut ret = RangeServerT::default();
        ret.server_id = value.server_id;
        ret.advertise_addr = value.advertise_address.clone();
        ret
    }
}

impl From<&RangeServerT> for RangeServer {
    fn from(value: &RangeServerT) -> Self {
        Self {
            server_id: value.server_id,
            advertise_address: value.advertise_addr.clone(),
        }
    }
}
