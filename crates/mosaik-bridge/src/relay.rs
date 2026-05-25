//! Nostr-side codec for the cross-chain [`SwapEnvelope`].
//!
//! The kind number lives in `mosaik-relay` (so all event-kind constants live
//! in one place — see `mosaik_relay::CROSS_CHAIN_SWAP_KIND`), but the encode
//! / decode for kind 30052 lives here so the relay crate doesn't have to
//! know about [`SwapEnvelope`].
//!
//! Tags published:
//!
//! | Tag | Value | Purpose |
//! |---|---|---|
//! | `d` | `swap_id` | Addressable identifier — re-publishing replaces. |
//! | `state` | `drafted` / `half_published` / `both_published` / `revealed` / `settled` / `refunded` | Lifecycle, mirrors the body. |
//! | `backend` | each leg's backend (two tags per swap) | Filterability. |
//! | `pair` | `<backend_a>:<outpoint_a>↔<backend_b>:<outpoint_b>` | Lets clients group swaps by venue pair. |
//! | `hashlock` | `preimage_hash` hex | Filterability across announcements that share a commitment. |
//! | `a_refund` / `b_refund` | unix seconds | Clients can filter out near-expiry swaps. |

use anyhow::{Context, Result};
use mosaik_relay::CROSS_CHAIN_SWAP_KIND;
use nostr::{Alphabet, Event, EventBuilder, Filter, Keys, Kind, SingleLetterTag, Tag, TagKind};

use crate::envelope::SwapEnvelope;

/// Wrapper that knows how to turn a [`SwapEnvelope`] into / out of a
/// Nostr event of kind [`mosaik_relay::CROSS_CHAIN_SWAP_KIND`].
pub struct SwapAnnouncement<'a>(pub &'a SwapEnvelope);

impl SwapAnnouncement<'_> {
    /// Build and sign the kind-30052 event for this envelope.
    pub fn to_event(&self, keys: &Keys) -> Result<Event> {
        let env = self.0;
        env.validate().map_err(anyhow::Error::msg)?;
        let content = serde_json::to_string(env).context("serialising swap envelope")?;

        let pair_tag = format!(
            "{}:{}↔{}:{}",
            env.leg_a.backend, env.leg_a.outpoint, env.leg_b.backend, env.leg_b.outpoint
        );

        let builder = EventBuilder::new(Kind::Custom(CROSS_CHAIN_SWAP_KIND), content)
            .tag(Tag::identifier(env.swap_id.clone()))
            .tag(custom_tag("state", env.state.label()))
            .tag(custom_tag("backend", &env.leg_a.backend))
            .tag(custom_tag("backend", &env.leg_b.backend))
            .tag(custom_tag("pair", &pair_tag))
            .tag(custom_tag("hashlock", &env.preimage_hash.to_hex()))
            .tag(custom_tag("a_refund", &env.timelocks.a_refund.to_string()))
            .tag(custom_tag("b_refund", &env.timelocks.b_refund.to_string()));

        builder
            .sign_with_keys(keys)
            .context("signing cross-chain swap event")
    }

    /// Parse a [`SwapEnvelope`] from a Nostr event.
    pub fn from_event(event: &Event) -> Result<SwapEnvelope> {
        if event.kind != Kind::Custom(CROSS_CHAIN_SWAP_KIND) {
            anyhow::bail!(
                "not a cross-chain swap event (kind {})",
                event.kind.as_u16()
            );
        }
        let env: SwapEnvelope =
            serde_json::from_str(&event.content).context("parsing swap envelope")?;
        env.validate().map_err(anyhow::Error::msg)?;
        Ok(env)
    }

    /// A Nostr filter that matches cross-chain swap events.
    pub fn filter() -> Filter {
        Filter::new().kind(Kind::Custom(CROSS_CHAIN_SWAP_KIND))
    }
}

fn custom_tag(name: &str, value: &str) -> Tag {
    // For known single-letter Nostr tags ("d") we use the dedicated path so
    // the relay indexes them; everything else is a custom multi-char tag
    // (informational, but searchable by clients that fetch the whole event).
    if name == "d" {
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)),
            [value.to_string()],
        )
    } else {
        Tag::custom(TagKind::custom(name.to_string()), [value.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{LegRef, SwapTimelocks};
    use crate::preimage::PreimageHash;
    use crate::state::SwapStateMachine;

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
    fn swap_announcement_roundtrips_through_a_nostr_event() {
        let keys = Keys::generate();
        let env = sample();

        let event = SwapAnnouncement(&env).to_event(&keys).unwrap();
        assert_eq!(event.kind, Kind::Custom(CROSS_CHAIN_SWAP_KIND));
        assert!(event.verify().is_ok());

        let back = SwapAnnouncement::from_event(&event).unwrap();
        assert_eq!(back.swap_id, env.swap_id);
        assert_eq!(back.preimage_hash, env.preimage_hash);
        assert_eq!(back.leg_a.backend, "liquid");
        assert_eq!(back.leg_b.backend, "banco");
    }

    #[test]
    fn swap_announcement_carries_filterable_tags() {
        let keys = Keys::generate();
        let event = SwapAnnouncement(&sample()).to_event(&keys).unwrap();
        let tag_values: Vec<String> = event.tags.iter().flat_map(|t| t.clone().to_vec()).collect();
        assert!(tag_values.iter().any(|v| v == "mosaik-swap-001"));
        assert!(tag_values.iter().any(|v| v == "both_published"));
        assert!(tag_values.iter().any(|v| v == "liquid"));
        assert!(tag_values.iter().any(|v| v == "banco"));
        // hashlock tag carries the hex of [0xab; 32]
        let hashlock_hex = "ab".repeat(32);
        assert!(tag_values.iter().any(|v| v == &hashlock_hex));
    }

    #[test]
    fn swap_announcement_rejects_other_event_kinds() {
        let keys = Keys::generate();
        let other = EventBuilder::new(Kind::TextNote, "hello")
            .sign_with_keys(&keys)
            .unwrap();
        assert!(SwapAnnouncement::from_event(&other).is_err());
    }

    #[test]
    fn swap_announcement_rejects_invalid_envelope() {
        let keys = Keys::generate();
        let mut env = sample();
        env.leg_b.backend = "liquid".into(); // duplicate backend
        let err = SwapAnnouncement(&env).to_event(&keys).unwrap_err();
        assert!(err.to_string().contains("same backend"));
    }
}
