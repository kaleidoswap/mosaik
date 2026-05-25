//! The wire shape of a cross-chain swap announcement.
//!
//! A [`SwapEnvelope`] is the addressable record the relay carries at kind
//! 30052. It references both single-leg `Offer`s by their relay event ids
//! and chain funding refs, and carries the shared [`PreimageHash`] plus
//! the negotiated timelocks. Spelled out so any client can validate the
//! swap is well-formed without watching every chain itself.

use mosaik_core::Offer;
use serde::{Deserialize, Serialize};

use crate::preimage::PreimageHash;
use crate::state::SwapStateMachine;

/// Identifier of one leg of a cross-chain swap on a particular backend.
///
/// `event_id` is the kind-30050 event that carries the underlying single-leg
/// offer; `outpoint` mirrors that offer's funding ref (Liquid txid:vout or
/// Arkade funding txid:vout). Carrying both lets a client verify
/// independently from the relay or independently from the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegRef {
    /// `"liquid"` or `"banco"`.
    pub backend: String,
    /// The funding reference (Liquid `txid:vout` or Arkade `txid:vout`).
    pub outpoint: String,
    /// The relay event id (kind 30050) that carries the underlying offer,
    /// or `None` if the leg is referenced by chain alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
}

/// The two timelocks a swap is parameterised by, in **unix seconds**.
///
/// The orchestrator enforces `b_refund + safety_margin <= a_refund` —
/// see `docs/CROSS_CHAIN_SWAP.md` §"Timelock-asymmetry math".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapTimelocks {
    /// Leg-A refund deadline. Must be the LONGER of the two.
    pub a_refund: u64,
    /// Leg-B refund deadline. Must be the SHORTER of the two.
    pub b_refund: u64,
    /// Minimum required gap `a_refund - b_refund` (seconds).
    pub safety_margin: u64,
}

impl SwapTimelocks {
    /// Build a sane default given `now` + a 1h-short / 2h-long window.
    pub fn default_from(now_unix: u64) -> Self {
        Self {
            a_refund: now_unix + 7200,
            b_refund: now_unix + 3600,
            safety_margin: 1800,
        }
    }

    /// Verify the asymmetry constraint. The orchestrator calls this on
    /// every state-changing op.
    pub fn validate(&self) -> Result<(), String> {
        if self.b_refund + self.safety_margin > self.a_refund {
            return Err(format!(
                "leg-B refund {} too close to leg-A refund {}: requires \
                 at least {}s of safety margin",
                self.b_refund, self.a_refund, self.safety_margin
            ));
        }
        Ok(())
    }
}

/// The kind-30052 envelope JSON. Pairs two `Offer`s and binds them with a
/// shared preimage commitment + timelocks. Lives at addressable id
/// `swap_id`; re-publishing under the same id updates the lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapEnvelope {
    /// Addressable identifier — `d` tag on the relay event.
    pub swap_id: String,
    /// `SHA256(preimage)` shared by both legs. Public; never carry the
    /// preimage itself in this envelope.
    pub preimage_hash: PreimageHash,
    /// Leg-A — by convention, the maker's chain (the one that picked the
    /// preimage). Has the LONGER refund deadline.
    pub leg_a: LegRef,
    /// Leg-B — the counterparty's chain. SHORTER refund deadline.
    pub leg_b: LegRef,
    /// Timelocks for both legs.
    pub timelocks: SwapTimelocks,
    /// Lifecycle state — re-publish to advance.
    pub state: SwapStateMachine,
    /// Optional full snapshot of the leg-A offer, for clients that don't
    /// want to dereference `leg_a.event_id` against the relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leg_a_offer: Option<Offer>,
    /// Optional full snapshot of the leg-B offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leg_b_offer: Option<Offer>,
}

impl SwapEnvelope {
    /// Sanity checks beyond serde — call before publishing.
    pub fn validate(&self) -> Result<(), String> {
        self.timelocks.validate()?;
        if self.leg_a.backend == self.leg_b.backend {
            return Err(format!(
                "both legs target the same backend ('{}'); a bridge swap requires two distinct chains",
                self.leg_a.backend
            ));
        }
        // Empty outpoints mean "not yet funded" — legal during the Drafted /
        // HalfPublished states. Only flag a collision once both legs claim to
        // be funded.
        if !self.leg_a.outpoint.is_empty() && self.leg_a.outpoint == self.leg_b.outpoint {
            return Err(
                "leg-A and leg-B share an outpoint; they must reference distinct funded UTXOs"
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SwapEnvelope {
        SwapEnvelope {
            swap_id: "mosaik-swap-001".into(),
            preimage_hash: PreimageHash([0xab; 32]),
            leg_a: LegRef {
                backend: "liquid".into(),
                outpoint: "aa".repeat(32) + ":0",
                event_id: Some("evt-a".into()),
            },
            leg_b: LegRef {
                backend: "banco".into(),
                outpoint: "bb".repeat(32) + ":1",
                event_id: Some("evt-b".into()),
            },
            timelocks: SwapTimelocks::default_from(1_700_000_000),
            state: SwapStateMachine::BothPublished,
            leg_a_offer: None,
            leg_b_offer: None,
        }
    }

    #[test]
    fn envelope_roundtrips_through_json() {
        let env = sample();
        env.validate().unwrap();
        let json = serde_json::to_string(&env).unwrap();
        let back: SwapEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.swap_id, env.swap_id);
        assert_eq!(back.preimage_hash, env.preimage_hash);
        assert_eq!(back.leg_a.backend, "liquid");
        assert_eq!(back.leg_b.backend, "banco");
    }

    #[test]
    fn validate_rejects_same_backend_on_both_legs() {
        let mut env = sample();
        env.leg_b.backend = "liquid".into();
        let err = env.validate().unwrap_err();
        assert!(err.contains("same backend"));
    }

    #[test]
    fn validate_rejects_same_outpoint_on_both_legs() {
        let mut env = sample();
        env.leg_b.outpoint = env.leg_a.outpoint.clone();
        let err = env.validate().unwrap_err();
        assert!(err.contains("share an outpoint"));
    }

    #[test]
    fn timelocks_default_satisfies_safety_margin() {
        SwapTimelocks::default_from(1_700_000_000)
            .validate()
            .unwrap();
    }

    #[test]
    fn timelocks_validate_rejects_tight_gap() {
        let bad = SwapTimelocks {
            a_refund: 1_000_100,
            b_refund: 1_000_000,
            safety_margin: 1800,
        };
        let err = bad.validate().unwrap_err();
        assert!(err.contains("safety margin"));
    }
}
