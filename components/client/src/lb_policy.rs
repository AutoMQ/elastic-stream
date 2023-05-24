/// Load-balancing policy among sessions within `CompositeSession`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub(crate) enum LbPolicy {
    PickFirst,
    LeaderOnly,
}
