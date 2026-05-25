//! Mosaik cross-chain covenant bridge.
//!
//! Architectural scaffolding for atomic asset swaps **Liquid-Tessera ↔
//! Arkade-banco**. Today this crate ships:
//!
//! - [`Preimage`] / [`PreimageHash`] — 32-byte secret + SHA-256 commitment.
//! - [`SwapEnvelope`] / [`LegRef`] / [`SwapTimelocks`] — the wire shape of
//!   a cross-chain swap announcement (kind 30052 on the relay).
//! - [`SwapStateMachine`] — exhaustively typed lifecycle. Invalid
//!   transitions are compile errors, not runtime checks.
//! - [`BridgeOrchestrator`] trait + [`MockOrchestrator`] in-memory impl that
//!   walks the full state machine for tests and integration scaffolding.
//!
//! The *on-chain primitive extensions* this crate's runtime path depends
//! on (a Tessera variant with a preimage-hashlock SETTLE branch, and a
//! banco preimage-gated fulfilment path) land in their own follow-up
//! commits. See `docs/CROSS_CHAIN_SWAP.md` §"Primitive gaps".
//!
//! Re-exports below let callers `use mosaik_bridge::*` without learning
//! the module layout.

pub mod envelope;
pub mod orchestrator;
pub mod preimage;
pub mod relay;
pub mod state;

pub use relay::SwapAnnouncement;

pub use envelope::{LegRef, SwapEnvelope, SwapTimelocks};
pub use orchestrator::{
    assert_primitives_available, banco_hashlock_gap, liquid_hashlock_gap, primitive_gap,
    BridgeError, BridgeOrchestrator, BridgeResult, LegBackend, MockOrchestrator, SwapTerms,
};
pub use preimage::{Preimage, PreimageHash};
pub use state::{RefundReason, SwapStateMachine};
