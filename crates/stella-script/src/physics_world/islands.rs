//! Native Box2D island traversal order captured for each fixed step.

use crate::ContactKey;

#[derive(Debug, Clone, Default)]
pub(crate) struct SolverIsland {
    /// Includes static endpoints exactly as b2Island::Add(body) does. Static
    /// bodies terminate the DFS and are ignored by integration/sleep.
    pub(crate) bodies: Vec<String>,
    /// Constraint order is the order in which the body's contact-edge lists
    /// were visited while assembling the island.
    pub(crate) contacts: Vec<ContactKey>,
    /// Constraint order is the order in which the body's joint-edge lists
    /// were visited while assembling the island.
    pub(crate) joints: Vec<String>,
}
