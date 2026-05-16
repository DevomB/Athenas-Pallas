//! Standalone working-order view (order manager surface without the full engine).
//!
//! The live engine still owns [`crate::state::GlobalState::open_orders`], which dereferences like a
//! `HashMap` via [`OrderStore`] for drop-in compatibility.

use crate::types::{OpenOrder, OrderId, OrderStatus};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

/// In-memory resting orders keyed by [`OrderId`].
#[derive(Clone, Debug, Default)]
pub struct OrderStore(pub HashMap<OrderId, OpenOrder>);

impl Deref for OrderStore {
    type Target = HashMap<OrderId, OpenOrder>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OrderStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl OrderStore {
    /// Empty book.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Merge a venue-style order update into the book (same rules as [`crate::state::GlobalState::apply_account`]).
    pub fn apply_order_update(&mut self, o: OpenOrder) {
        if matches!(
            o.status,
            OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Rejected
        ) {
            self.0.remove(&o.id);
        } else {
            self.0.insert(o.id.clone(), o);
        }
    }

    /// Clone list of working orders (open / pending).
    pub fn working_orders(&self) -> Vec<OpenOrder> {
        self.0.values().cloned().collect()
    }
}
