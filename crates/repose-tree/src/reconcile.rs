//! Reconciliation context and utilities.

/// Context for a reconciliation pass.
pub struct ReconcileContext {
    /// Current generation.
    pub generation: u64,

    /// Count of nodes that were reconciled (updated).
    pub reconciled: usize,

    /// Count of nodes that were skipped (unchanged).
    pub skipped: usize,

    /// Count of nodes that were created.
    pub created: usize,

    /// Count of nodes that were removed.
    pub removed: usize,
}

impl ReconcileContext {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            reconciled: 0,
            skipped: 0,
            created: 0,
            removed: 0,
        }
    }
}
