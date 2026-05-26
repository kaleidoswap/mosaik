//! `RealOrchestrator` — the on-chain bridge implementation.
//!
//! This is the orchestrator the platform will run in production. It calls
//! `mosaik_core::MosaikMaker` / `mosaik_core::MosaikTaker` for the Liquid
//! leg and `mosaik_banco::BancoMaker` / `mosaik_banco::BancoTaker` for the
//! Arkade-banco leg. The day the two on-chain primitive extensions in
//! `docs/CROSS_CHAIN_SWAP.md` §"Primitive gaps" close — a Tessera variant
//! with a preimage-hashlock SETTLE branch (Gap A) and a banco preimage-
//! gated fulfilment path (Gap B) — every `PrimitiveGap` site below
//! becomes a concrete call against the backend client.
//!
//! What works today, against an actually-running Liquid regtest and a
//! banco-proxy with a real wallet seed:
//!
//! | Method            | Status                                            |
//! |-------------------|---------------------------------------------------|
//! | `draft`           | ✅ Full — preimage generation, envelope build     |
//! | `publish_leg_a`   | ❌ Gap A — needs `tessera-hashlock.simf`          |
//! | `publish_leg_b`   | ❌ Gap B — needs banco preimage-gated fulfilment  |
//! | `await_preimage`  | ❌ Both gaps — chain watcher for the reveal       |
//! | `complete`        | ❌ Both gaps — settles the surviving leg          |
//! | `refund`          | ⚠️  Partial — uses existing (non-hashlocked) refund paths so the orchestrator is at least *reclaimable* today; full Liquid hashlock-refund + banco-cancel land with their gaps |
//!
//! In other words: drafting + refunding are wired against real backends now;
//! the SHA-256-binding parts of the protocol return clean `PrimitiveGap`
//! errors with the spec URL in the message. Callers (mosaik-api, the CLI)
//! see actionable failures pointing at the design doc.

use mosaik_banco::{BancoClient, BancoMaker};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{MosaikMaker, OfferKind, ReclaimOffer};

use crate::envelope::{LegRef, SwapEnvelope};
use crate::orchestrator::{
    banco_hashlock_gap, liquid_hashlock_gap, BridgeError, BridgeOrchestrator, BridgeResult,
    LegBackend, SwapTerms,
};
use crate::preimage::Preimage;
use crate::state::{RefundReason, SwapStateMachine};

/// Per-leg backend client. Distinct shapes (Elements RPC for Liquid, an HTTP
/// gateway for banco) so each leg can be wired or left absent independently.
#[derive(Clone)]
pub enum LegClient {
    Liquid {
        /// Connection to an `elementsd` regtest / mainnet that can settle /
        /// reclaim Tessera covenants.
        rpc: ElementsRpc,
    },
    Banco {
        /// Connection to `banco-proxy` for the Arkade side. Stored by value
        /// (not Arc) because `BancoClient` is `!Sync` (it caches a JWT in
        /// a `RefCell`) — the orchestrator is single-threaded by construction;
        /// cross-thread sharing would need a different wrapper.
        client: BancoClient,
    },
}

impl LegClient {
    fn backend(&self) -> LegBackend {
        match self {
            LegClient::Liquid { .. } => LegBackend::Liquid,
            LegClient::Banco { .. } => LegBackend::Banco,
        }
    }
}

/// The real-chain bridge orchestrator. Holds one client per leg and the
/// preimage drafted at swap-creation time.
pub struct RealOrchestrator {
    leg_a: LegClient,
    leg_b: LegClient,
    /// Last preimage drafted by `draft`. The maker holds this until the swap
    /// settles (it's only ever revealed on-chain via a leg-claim spend).
    preimage: Option<Preimage>,
}

impl RealOrchestrator {
    /// Build an orchestrator from the two leg clients. Order matters: leg-A
    /// is the *longer-refund* leg (the maker's side). The two clients MUST
    /// target distinct backends — the same constraint the state machine
    /// enforces at draft time.
    pub fn new(leg_a: LegClient, leg_b: LegClient) -> Self {
        Self {
            leg_a,
            leg_b,
            preimage: None,
        }
    }

    /// Build with Liquid as leg-A and banco as leg-B — the canonical
    /// Mosaik bridge configuration.
    pub fn liquid_banco(elements: ElementsRpc, banco: BancoClient) -> Self {
        Self::new(
            LegClient::Liquid { rpc: elements },
            LegClient::Banco { client: banco },
        )
    }
}

impl BridgeOrchestrator for RealOrchestrator {
    fn draft(&mut self, terms: SwapTerms) -> BridgeResult<(SwapEnvelope, Preimage)> {
        terms
            .timelocks
            .validate()
            .map_err(BridgeError::InvalidTimelocks)?;

        let actual_a = self.leg_a.backend();
        let actual_b = self.leg_b.backend();
        if terms.leg_a_backend != actual_a {
            return Err(BridgeError::InvalidTransition(format!(
                "leg-A backend mismatch: terms ask for '{}' but client is '{}'",
                terms.leg_a_backend.label(),
                actual_a.label(),
            )));
        }
        if terms.leg_b_backend != actual_b {
            return Err(BridgeError::InvalidTransition(format!(
                "leg-B backend mismatch: terms ask for '{}' but client is '{}'",
                terms.leg_b_backend.label(),
                actual_b.label(),
            )));
        }

        let preimage = Preimage::random();
        let hash = preimage.hash();
        self.preimage = Some(preimage.clone());

        let envelope = SwapEnvelope {
            swap_id: terms.swap_id,
            preimage_hash: hash,
            leg_a: LegRef {
                backend: terms.leg_a_backend.label().to_string(),
                outpoint: String::new(),
                event_id: None,
            },
            leg_b: LegRef {
                backend: terms.leg_b_backend.label().to_string(),
                outpoint: String::new(),
                event_id: None,
            },
            timelocks: terms.timelocks,
            state: SwapStateMachine::Drafted,
            leg_a_offer: None,
            leg_b_offer: None,
        };
        Ok((envelope, preimage))
    }

    fn publish_leg_a(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef> {
        require_state(&envelope.state, SwapStateMachine::Drafted, "publish_leg_a")?;
        // Funding leg-A requires the maker's covenant to commit to the
        // preimage hash. Today's Tessera (crates/tessera/contracts/
        // tessera.simf) has SETTLE + REFUND only — no hashlock branch. The
        // call below stays unreached until Gap A lands.
        let _ = &self.leg_a; // suppress unused on the typed receiver
        Err(liquid_hashlock_gap())
    }

    fn publish_leg_b(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef> {
        require_state(
            &envelope.state,
            SwapStateMachine::HalfPublished,
            "publish_leg_b",
        )?;
        // Funding leg-B requires banco's covenant to gate fulfilment on a
        // preimage reveal. `@arkade-os/banco`'s public API doesn't expose
        // that path today (see docs/CROSS_CHAIN_SWAP.md §"Primitive gaps").
        let _ = &self.leg_b;
        Err(banco_hashlock_gap())
    }

    fn await_preimage(
        &mut self,
        envelope: &mut SwapEnvelope,
        _timeout_secs: u64,
    ) -> BridgeResult<Preimage> {
        require_state(
            &envelope.state,
            SwapStateMachine::BothPublished,
            "await_preimage",
        )?;
        // Watching for the reveal means subscribing to whichever leg gets
        // claimed first, parsing the spending tx's witness, and recovering
        // the preimage. Both backends need their hashlock primitive in
        // place before this is a meaningful operation.
        Err(liquid_hashlock_gap())
    }

    fn complete(
        &mut self,
        envelope: &mut SwapEnvelope,
        _preimage: &Preimage,
    ) -> BridgeResult<String> {
        require_state(&envelope.state, SwapStateMachine::Revealed, "complete")?;
        // Spending the surviving leg with the revealed preimage; same gap
        // as publish_leg_a / publish_leg_b — needs the on-chain hashlock
        // primitive on at least one backend.
        Err(liquid_hashlock_gap())
    }

    fn refund(&mut self, envelope: &mut SwapEnvelope, reason: RefundReason) -> BridgeResult<()> {
        if matches!(envelope.state, SwapStateMachine::Settled) {
            return Err(BridgeError::InvalidTransition(
                "cannot refund a settled swap".into(),
            ));
        }

        // Best-effort refund: each leg that was funded gets reclaimed via the
        // existing (non-hashlocked) refund path. Both legs are independent —
        // if one fails (e.g. only leg-A was funded), the other call no-ops
        // because the envelope's outpoint for that leg is empty.
        //
        // The Liquid Tessera's keyless REFUND branch and banco's CLTV cancel
        // both work today; they're the same primitives we use in the
        // single-chain offer paths. What's NOT here yet: a hashlocked
        // refund that would let the *other* party reclaim the dangling leg
        // after timeout — that requires Gap A / Gap B.
        if !envelope.leg_a.outpoint.is_empty() {
            if let Some(leg_a_offer) = envelope.leg_a_offer.as_ref() {
                if let LegClient::Liquid { rpc } = &self.leg_a {
                    if matches!(leg_a_offer.kind, OfferKind::Liquid(_)) {
                        // The keyless REFUND path on Tessera.
                        let maker = MosaikMaker::new(rpc.clone());
                        maker
                            .reclaim(leg_a_offer)
                            .map_err(|e| BridgeError::Transport(format!("leg-A refund: {e}")))?;
                    }
                }
            }
        }
        if !envelope.leg_b.outpoint.is_empty() {
            if let Some(leg_b_offer) = envelope.leg_b_offer.as_ref() {
                if let LegClient::Banco { client } = &self.leg_b {
                    if matches!(leg_b_offer.kind, OfferKind::Banco(_)) {
                        BancoMaker::new(client.clone())
                            .cancel(leg_b_offer)
                            .map_err(|e| BridgeError::Transport(format!("leg-B cancel: {e}")))?;
                    }
                }
            }
        }

        envelope.state = SwapStateMachine::Refunded { reason };
        Ok(())
    }
}

/// Reject a transition whose receiver-state doesn't match expectations.
/// Mirrors the helper in `orchestrator::MockOrchestrator` so the two
/// implementations surface identical error shapes.
fn require_state(
    actual: &SwapStateMachine,
    expected: SwapStateMachine,
    op: &str,
) -> BridgeResult<()> {
    if actual.label() == expected.label() {
        Ok(())
    } else {
        Err(BridgeError::InvalidTransition(format!(
            "{op}: swap is '{}', expected '{}'",
            actual.label(),
            expected.label()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::SwapTimelocks;

    /// Build a RealOrchestrator with two distinct (but inert) leg clients
    /// — enough to exercise the draft + state-machine guard paths.
    fn orch() -> RealOrchestrator {
        let elements = ElementsRpc::wallet("http://127.0.0.1:9", "wallet", "u", "p");
        let banco = BancoClient::new("http://127.0.0.1:9", "test");
        RealOrchestrator::liquid_banco(elements, banco)
    }

    fn terms() -> SwapTerms {
        SwapTerms {
            swap_id: "real-test".into(),
            leg_a_backend: LegBackend::Liquid,
            leg_b_backend: LegBackend::Banco,
            timelocks: SwapTimelocks::default_from(1_700_000_000),
        }
    }

    #[test]
    fn draft_succeeds_with_matching_backends() {
        let mut o = orch();
        let (env, _p) = o.draft(terms()).expect("draft should succeed");
        assert_eq!(env.swap_id, "real-test");
        assert_eq!(env.leg_a.backend, "liquid");
        assert_eq!(env.leg_b.backend, "banco");
        assert_eq!(env.state.label(), "drafted");
    }

    #[test]
    fn draft_rejects_mismatched_leg_backends() {
        let mut o = orch();
        let mut t = terms();
        t.leg_a_backend = LegBackend::Banco; // swap; client has Liquid in slot A
        let err = o.draft(t).unwrap_err();
        assert!(matches!(err, BridgeError::InvalidTransition(_)));
    }

    #[test]
    fn publish_leg_a_returns_primitive_gap_pointing_at_doc() {
        let mut o = orch();
        let (mut env, _) = o.draft(terms()).unwrap();
        let err = o.publish_leg_a(&mut env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CROSS_CHAIN_SWAP.md"));
        assert!(msg.contains("Gap A"));
    }

    #[test]
    fn publish_leg_b_returns_primitive_gap_pointing_at_doc() {
        let mut o = orch();
        let (mut env, _) = o.draft(terms()).unwrap();
        // Force the state forward as if leg-A had landed, so the state
        // guard lets us hit the leg-B publish path's gap error rather
        // than the state-mismatch rejection.
        env.state = SwapStateMachine::HalfPublished;
        let err = o.publish_leg_b(&mut env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Gap B"));
    }

    #[test]
    fn refund_from_drafted_state_clears_to_refunded() {
        let mut o = orch();
        let (mut env, _) = o.draft(terms()).unwrap();
        // Nothing on-chain → no per-leg reclaim runs → state flip alone.
        o.refund(&mut env, RefundReason::NeverPublished).unwrap();
        assert!(matches!(env.state, SwapStateMachine::Refunded { .. }));
    }

    #[test]
    fn refund_rejects_a_settled_swap() {
        let mut o = orch();
        let (mut env, _) = o.draft(terms()).unwrap();
        env.state = SwapStateMachine::Settled;
        let err = o
            .refund(&mut env, RefundReason::NeverPublished)
            .unwrap_err();
        assert!(matches!(err, BridgeError::InvalidTransition(_)));
    }
}
