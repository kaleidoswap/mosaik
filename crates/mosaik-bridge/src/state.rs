//! State machine for a cross-chain bridge swap.
//!
//! Invalid transitions are compile errors — every method on
//! [`SwapStateMachine`] is gated by the state-typed receiver. There is no
//! string-typed `state` field that an off-by-one can land in.
//!
//! See `docs/CROSS_CHAIN_SWAP.md` §"State machine" for the diagram.
//!
//! ```
//! use mosaik_bridge::state::{SwapStateMachine, RefundReason};
//!
//! let drafted  = SwapStateMachine::draft();
//! let half     = drafted.publish_leg_a();
//! let both     = half.publish_leg_b();
//! let revealed = both.observe_preimage();
//! let settled  = revealed.complete_second_leg();
//! assert_eq!(settled.snapshot().label(), "settled");
//!
//! // Refund-on-deadline path:
//! let half = SwapStateMachine::draft().publish_leg_a();
//! let refunded = half.refund(RefundReason::PartnerNeverPublished);
//! assert_eq!(refunded.snapshot().label(), "refunded");
//! ```

use serde::{Deserialize, Serialize};

/// Why a swap exited to the terminal `Refunded` state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundReason {
    /// `Drafted` → `Refunded`: maker abandoned before any leg was funded.
    NeverPublished,
    /// `HalfPublished` → `Refunded`: counterparty never funded the second leg.
    PartnerNeverPublished,
    /// `BothPublished` → `Refunded`: nobody ever fulfilled either leg; both
    /// legs refunded independently after their respective deadlines.
    NoTakerArrived,
    /// `Revealed` → `Refunded`: pathological case — preimage was revealed but
    /// the maker missed the window to claim the other leg.
    MakerMissedClaimWindow,
}

/// The lifecycle of a cross-chain swap, exhaustively typed.
///
/// Use [`SwapStateMachine::draft`] to start; every legal transition is a
/// method on the current-state type and returns the next-state type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SwapStateMachine {
    Drafted,
    HalfPublished,
    BothPublished,
    Revealed,
    Settled,
    Refunded { reason: RefundReason },
}

impl SwapStateMachine {
    /// Start a fresh swap. Maker has picked terms + a preimage but nothing
    /// is on-chain yet.
    pub fn draft() -> Drafted {
        Drafted
    }

    /// Short human label for logs / event tags.
    pub fn label(&self) -> &'static str {
        match self {
            SwapStateMachine::Drafted => "drafted",
            SwapStateMachine::HalfPublished => "half_published",
            SwapStateMachine::BothPublished => "both_published",
            SwapStateMachine::Revealed => "revealed",
            SwapStateMachine::Settled => "settled",
            SwapStateMachine::Refunded { .. } => "refunded",
        }
    }

    /// True if the swap reached a terminal state — no more transitions.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SwapStateMachine::Settled | SwapStateMachine::Refunded { .. }
        )
    }
}

// ─── State-typed phantom shells ──────────────────────────────────────────────
// Each is a zero-sized type whose methods enumerate the legal exits.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drafted;
impl Drafted {
    /// Maker has just funded leg-A on-chain.
    pub fn publish_leg_a(self) -> HalfPublished {
        HalfPublished
    }
    /// Abandon before publishing anything.
    pub fn refund(self, reason: RefundReason) -> Refunded {
        Refunded { reason }
    }
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::Drafted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfPublished;
impl HalfPublished {
    /// Counterparty has funded the matching leg-B. The swap is now atomic.
    pub fn publish_leg_b(self) -> BothPublished {
        BothPublished
    }
    /// Counterparty never showed up; reclaim leg-A at its refund deadline.
    pub fn refund(self, reason: RefundReason) -> Refunded {
        Refunded { reason }
    }
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::HalfPublished
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BothPublished;
impl BothPublished {
    /// Someone spent one leg, revealing the preimage on-chain.
    pub fn observe_preimage(self) -> Revealed {
        Revealed
    }
    /// Nobody ever fulfilled either leg; both refund independently.
    pub fn refund(self, reason: RefundReason) -> Refunded {
        Refunded { reason }
    }
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::BothPublished
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revealed;
impl Revealed {
    /// The maker (or anyone) claimed the second leg with the revealed preimage.
    pub fn complete_second_leg(self) -> Settled {
        Settled
    }
    /// Pathological: the preimage was revealed but the second-leg claim
    /// window expired without the maker acting. See `CROSS_CHAIN_SWAP.md`
    /// §"Failure modes" — counts as the maker's loss.
    pub fn refund(self, reason: RefundReason) -> Refunded {
        Refunded { reason }
    }
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::Revealed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled;
impl Settled {
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::Settled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refunded {
    pub reason: RefundReason,
}
impl Refunded {
    pub fn snapshot(self) -> SwapStateMachine {
        SwapStateMachine::Refunded {
            reason: self.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_walks_to_settled() {
        let s = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .observe_preimage()
            .complete_second_leg()
            .snapshot();
        assert_eq!(s, SwapStateMachine::Settled);
        assert!(s.is_terminal());
        assert_eq!(s.label(), "settled");
    }

    #[test]
    fn drafted_can_refund_immediately() {
        let s = SwapStateMachine::draft()
            .refund(RefundReason::NeverPublished)
            .snapshot();
        assert!(matches!(
            s,
            SwapStateMachine::Refunded {
                reason: RefundReason::NeverPublished
            }
        ));
        assert!(s.is_terminal());
    }

    #[test]
    fn each_intermediate_state_can_refund() {
        let s = SwapStateMachine::draft()
            .publish_leg_a()
            .refund(RefundReason::PartnerNeverPublished)
            .snapshot();
        assert!(matches!(s, SwapStateMachine::Refunded { .. }));

        let s = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .refund(RefundReason::NoTakerArrived)
            .snapshot();
        assert!(matches!(s, SwapStateMachine::Refunded { .. }));

        let s = SwapStateMachine::draft()
            .publish_leg_a()
            .publish_leg_b()
            .observe_preimage()
            .refund(RefundReason::MakerMissedClaimWindow)
            .snapshot();
        assert!(matches!(s, SwapStateMachine::Refunded { .. }));
    }

    #[test]
    fn snapshots_roundtrip_through_json() {
        let s = SwapStateMachine::Refunded {
            reason: RefundReason::NoTakerArrived,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: SwapStateMachine = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn labels_are_stable() {
        // Lock in the wire labels — relay event tags depend on these.
        assert_eq!(SwapStateMachine::Drafted.label(), "drafted");
        assert_eq!(SwapStateMachine::HalfPublished.label(), "half_published");
        assert_eq!(SwapStateMachine::BothPublished.label(), "both_published");
        assert_eq!(SwapStateMachine::Revealed.label(), "revealed");
        assert_eq!(SwapStateMachine::Settled.label(), "settled");
        assert_eq!(
            SwapStateMachine::Refunded {
                reason: RefundReason::NeverPublished,
            }
            .label(),
            "refunded",
        );
    }
}
