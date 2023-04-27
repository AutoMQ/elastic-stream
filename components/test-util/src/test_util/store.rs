use store::ElasticStore;
use tokio::sync::oneshot;

pub fn build_store(pm_address: String, store_path: &str) -> ElasticStore {
    let mut cfg = config::Configuration::default();
    cfg.placement_manager = pm_address;
    cfg.store.path.set_base(store_path);
    cfg.check_and_apply().expect("Configuration is invalid");
    let (recovery_completion_tx, recovery_completion_rx) = oneshot::channel();
    let store = match ElasticStore::new(cfg, recovery_completion_tx) {
        Ok(store) => store,
        Err(e) => {
            panic!("Failed to launch ElasticStore: {:?}", e);
        }
    };

    recovery_completion_rx
        .blocking_recv()
        .expect("Await recovery completion");

    store
}
