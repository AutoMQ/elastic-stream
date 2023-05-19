pub trait Batch {
    /// Base offset of record batch
    fn offset(&self) -> u64;

    /// Quantity of the internal nested records
    fn len(&self) -> usize;

    /// Indicate whether the batch is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
