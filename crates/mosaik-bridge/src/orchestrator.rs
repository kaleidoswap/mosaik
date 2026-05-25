//! Bridge orchestrator — the coordination layer over two single-leg backends.
//!
//! Today this is a *scaffold*: the real `publish_leg_a` / `publish_leg_b` /
//! `claim_with_preimage` paths return [`BridgeError::PrimitiveGap`], pointing
//! at the two on-chain primitive extensions documented in
//! `docs/CROSS_CHAIN_SWAP.md` §"Primitive gaps":
//!
//!   A. `tessera-hashlock.simf` — a Tessera covenant variant whose SETTLE
//!      branch requires revealing `SHA256(preimage) == HASHLOCK` in addition
//!      to the existing introspection rules.
//!   B. banco preimage-gated fulfilment — composing the existing banco
//!      covenant with a SHA-256 preimage tap leaf, plus a `preimageHash`
//!      param on `POST /banco/maker/offer` and a reveal-extracting endpoint
//!      on the banco-proxy.
//!
//! Both gaps are scoped + estimated in the design doc. When they land, every
//! `PrimitiveGap` site in this file becomes a concrete call against
//! `mosaik-core` / `mosaik-banco` — the types and the state machine here are
//! already correct.
//!
//! The [`MockOrchestrator`] does walk the full state machine end-to-end in
//! memory, so the data flow and the [`SwapStateMachine`] are exercised by
//! tests today.

use crate::envelope::{LegRef, SwapEnvelope, SwapTimelocks};
use crate::preimage::Preimage;
use crate::state::{RefundReason, SwapStateMachine};

/// Errors a bridge orchestrator can surface.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// One of the two on-chain primitive extensions is not yet implemented.
    /// See `docs/CROSS_CHAIN_SWAP.md` §"Primitive gaps".
    #[error(
        "this code path requires the '{gap}' primitive extension, which is \
         not implemented yet. See docs/CROSS_CHAIN_SWAP.md §'Primitive gaps'."
    )]
    PrimitiveGap { gap: &'static str },

    /// Caller violated the timelock-asymmetry constraint.
    #[error("invalid timelocks: {0}")]
    InvalidTimelocks(String),

    /// The state machine refused a transition.
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),

    /// Underlying I/O / chain / proxy failure.
    #[error("transport: {0}")]
    Transport(String),
}

pub type BridgeResult<T> = Result<T, BridgeError>;

/// Per-leg backend identifier, used to pick which gap-error to surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegBackend {
    Liquid,
    Banco,
}

impl LegBackend {
    pub fn label(self) -> &'static str {
        match self {
            LegBackend::Liquid => "liquid",
            LegBackend::Banco => "banco",
        }
    }
}

/// Inputs the orchestrator needs to draft a fresh swap.
#[derive(Debug, Clone)]
pub struct SwapTerms {
    pub swap_id: String,
    /// Maker's side (publishes leg-A with the LONGER refund).
    pub leg_a_backend: LegBackend,
    /// Counterparty's side (publishes leg-B with the SHORTER refund).
    pub leg_b_backend: LegBackend,
    pub timelocks: SwapTimelocks,
}

/// The bridge orchestrator interface. Implementations:
///
/// - [`MockOrchestrator`] — drives the state machine in memory; useful in
///   tests and as the integration-target stub for clients.
/// - `RealOrchestrator` (TODO, lands with the primitive extensions) — calls
///   into `mosaik-core` for the Liquid leg and `mosaik-banco` for the banco
///   leg; surfaces real txids and watches both chains for the preimage reveal.
pub trait BridgeOrchestrator {
    /// Pick a preimage, validate terms, return the drafted envelope. No
    /// on-chain activity yet.
    fn draft(&mut self, terms: SwapTerms) -> BridgeResult<(SwapEnvelope, Preimage)>;

    /// Fund leg-A on its chain. Returns the leg-A outpoint and updates the
    /// envelope's state to `HalfPublished`.
    fn publish_leg_a(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef>;

    /// Fund leg-B on its chain. Updates the envelope to `BothPublished`.
    fn publish_leg_b(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef>;

    /// Watch the chain for a preimage reveal on either leg. Returns the
    /// revealed preimage once observed. Blocks (or polls internally).
    fn await_preimage(
        &mut self,
        envelope: &mut SwapEnvelope,
        timeout_secs: u64,
    ) -> BridgeResult<Preimage>;

    /// Claim the second leg by submitting the revealed preimage. Updates
    /// envelope state to `Settled`.
    fn complete(
        &mut self,
        envelope: &mut SwapEnvelope,
        preimage: &Preimage,
    ) -> BridgeResult<String>;

    /// Refund whichever leg(s) we funded. Updates envelope state to
    /// `Refunded { reason }`.
    fn refund(&mut self, envelope: &mut SwapEnvelope, reason: RefundReason) -> BridgeResult<()>;
}

// ─── Mock orchestrator (in-memory, exercises the state machine) ─────────────

/// In-memory orchestrator that drives the state machine without touching any
/// chain. Used to exercise the data flow in tests and to let clients build
/// against the real interface before the primitive gaps close.
#[derive(Debug, Default)]
pub struct MockOrchestrator {
    /// Last drafted preimage, returned by `await_preimage` to simulate
    /// observation. In a real orchestrator this comes from chain watching.
    preimage: Option<Preimage>,
}

impl MockOrchestrator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BridgeOrchestrator for MockOrchestrator {
    fn draft(&mut self, terms: SwapTerms) -> BridgeResult<(SwapEnvelope, Preimage)> {
        terms
            .timelocks
            .validate()
            .map_err(BridgeError::InvalidTimelocks)?;
        if terms.leg_a_backend == terms.leg_b_backend {
            return Err(BridgeError::InvalidTimelocks(
                "leg_a_backend and leg_b_backend must differ".into(),
            ));
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
            state: SwapStateMachine::draft().snapshot(),
            leg_a_offer: None,
            leg_b_offer: None,
        };
        Ok((envelope, preimage))
    }

    fn publish_leg_a(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef> {
        require_state(&envelope.state, SwapStateMachine::Drafted, "publish_leg_a")?;
        envelope.leg_a.outpoint = format!("mock-leg-a-{}", envelope.swap_id);
        envelope.state = SwapStateMachine::draft().publish_leg_a().snapshot();
        Ok(envelope.leg_a.clone())
    }

    fn publish_leg_b(&mut self, envelope: &mut SwapEnvelope) -> BridgeResult<LegRef> {
        require_state(
            &envelope.state,
            SwapStateMachine::HalfPublished,
            "publish_leg_b",
        )?;
        envelope.leg_b.outpoint = format!("mock-leg-b-{}", envelope.swap_id);
        envelope.state = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .snapshot();
        Ok(envelope.leg_b.clone())
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
        let p = self
            .preimage
            .clone()
            .ok_or_else(|| BridgeError::Transport("mock has no preimage to reveal".into()))?;
        if !envelope.preimage_hash.matches(&p) {
            return Err(BridgeError::Transport(
                "mock preimage does not match envelope hash".into(),
            ));
        }
        envelope.state = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .observe_preimage()
            .snapshot();
        Ok(p)
    }

    fn complete(
        &mut self,
        envelope: &mut SwapEnvelope,
        preimage: &Preimage,
    ) -> BridgeResult<String> {
        require_state(&envelope.state, SwapStateMachine::Revealed, "complete")?;
        if !envelope.preimage_hash.matches(preimage) {
            return Err(BridgeError::Transport(
                "preimage does not hash to the envelope's commitment".into(),
            ));
        }
        envelope.state = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .observe_preimage()
            .complete_second_leg()
            .snapshot();
        Ok(format!("mock-settle-tx-{}", envelope.swap_id))
    }

    fn refund(&mut self, envelope: &mut SwapEnvelope, reason: RefundReason) -> BridgeResult<()> {
        if matches!(envelope.state, SwapStateMachine::Settled) {
            return Err(BridgeError::InvalidTransition(
                "cannot refund a settled swap".into(),
            ));
        }
        envelope.state = SwapStateMachine::Refunded { reason };
        Ok(())
    }
}

fn require_state(
    actual: &SwapStateMachine,
    expected: SwapStateMachine,
    op: &str,
) -> BridgeResult<()> {
    // `expected` is supplied as a variant marker (e.g. SwapStateMachine::Drafted);
    // we compare label-equality so Refunded{reason:_} matches generically.
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

/// Helper: produce a `PrimitiveGap` error for the named gap. Used by the
/// real orchestrator stub (and by callers that want to surface the gap
/// without depending on this crate's variant constructors directly).
pub fn primitive_gap(gap: &'static str) -> BridgeError {
    BridgeError::PrimitiveGap { gap }
}

/// Used by the future `RealOrchestrator` Liquid leg.
pub fn liquid_hashlock_gap() -> BridgeError {
    primitive_gap("tessera-hashlock.simf (Gap A)")
}

/// Used by the future `RealOrchestrator` banco leg.
pub fn banco_hashlock_gap() -> BridgeError {
    primitive_gap("banco preimage-gated fulfilment (Gap B)")
}

/// Convenience wrapper that anyone (CLI, mosaik-api, future orchestrator)
/// can call to know whether a given (`leg_a_backend`, `leg_b_backend`) is
/// served by today's primitives. Always returns `Err` until Phase 8 lands.
pub fn assert_primitives_available(leg_a: LegBackend, leg_b: LegBackend) -> BridgeResult<()> {
    match (leg_a, leg_b) {
        // Liquid + banco is the canonical Phase 8 swap. Both gaps need to
        // close before we can claim the primitives are ready.
        (LegBackend::Liquid, LegBackend::Banco) | (LegBackend::Banco, LegBackend::Liquid) => Err(
            primitive_gap("Liquid-Tessera ↔ Arkade-banco — both Gap A and Gap B must land"),
        ),
        _ => Err(primitive_gap("two distinct backends required")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> SwapTerms {
        SwapTerms {
            swap_id: "swap-test-01".into(),
            leg_a_backend: LegBackend::Liquid,
            leg_b_backend: LegBackend::Banco,
            timelocks: SwapTimelocks::default_from(1_700_000_000),
        }
    }

    #[test]
    fn mock_walks_full_happy_path() {
        let mut o = MockOrchestrator::new();
        let (mut env, _preimage) = o.draft(terms()).unwrap();
        assert_eq!(env.state.label(), "drafted");

        o.publish_leg_a(&mut env).unwrap();
        assert_eq!(env.state.label(), "half_published");
        assert!(env.leg_a.outpoint.starts_with("mock-leg-a"));

        o.publish_leg_b(&mut env).unwrap();
        assert_eq!(env.state.label(), "both_published");

        let p = o.await_preimage(&mut env, 60).unwrap();
        assert_eq!(env.state.label(), "revealed");
        assert!(env.preimage_hash.matches(&p));

        let txid = o.complete(&mut env, &p).unwrap();
        assert_eq!(env.state.label(), "settled");
        assert!(txid.contains("mock-settle-tx"));
    }

    #[test]
    fn mock_refund_from_half_published() {
        let mut o = MockOrchestrator::new();
        let (mut env, _) = o.draft(terms()).unwrap();
        o.publish_leg_a(&mut env).unwrap();
        o.refund(&mut env, RefundReason::PartnerNeverPublished)
            .unwrap();
        assert!(matches!(env.state, SwapStateMachine::Refunded { .. }));
    }

    #[test]
    fn mock_rejects_out_of_order_transitions() {
        let mut o = MockOrchestrator::new();
        let (mut env, _) = o.draft(terms()).unwrap();
        // Skip publish_leg_a → publish_leg_b refuses.
        let err = o.publish_leg_b(&mut env).unwrap_err();
        assert!(matches!(err, BridgeError::InvalidTransition(_)));
    }

    #[test]
    fn mock_rejects_settled_refund() {
        let mut o = MockOrchestrator::new();
        let (mut env, _) = o.draft(terms()).unwrap();
        o.publish_leg_a(&mut env).unwrap();
        o.publish_leg_b(&mut env).unwrap();
        let p = o.await_preimage(&mut env, 60).unwrap();
        o.complete(&mut env, &p).unwrap();
        let err = o
            .refund(&mut env, RefundReason::NoTakerArrived)
            .unwrap_err();
        assert!(matches!(err, BridgeError::InvalidTransition(_)));
    }

    #[test]
    fn mock_rejects_same_backend_on_both_legs() {
        let mut o = MockOrchestrator::new();
        let mut t = terms();
        t.leg_b_backend = LegBackend::Liquid;
        let err = o.draft(t).unwrap_err();
        assert!(matches!(err, BridgeError::InvalidTimelocks(_)));
    }

    #[test]
    fn primitive_gap_messages_point_at_the_design_doc() {
        let e = liquid_hashlock_gap().to_string();
        assert!(e.contains("CROSS_CHAIN_SWAP.md"));
        let e = banco_hashlock_gap().to_string();
        assert!(e.contains("CROSS_CHAIN_SWAP.md"));
    }

    #[test]
    fn assert_primitives_available_currently_unavailable() {
        assert!(assert_primitives_available(LegBackend::Liquid, LegBackend::Banco).is_err());
        assert!(assert_primitives_available(LegBackend::Banco, LegBackend::Liquid).is_err());
    }
}
