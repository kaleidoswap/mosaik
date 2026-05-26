import type { MakerProfile } from '../api/types';

interface Props {
  profile?: MakerProfile;
}

/**
 * Inline maker-reputation chip rendered next to the maker name in the
 * order book. Stays compact: just the badges and the fill-latency hint.
 * Full profile detail (about, contact, pairs) lives on hover via the
 * native `title` attribute today; a dedicated detail popover is a
 * follow-up.
 */
export function MakerProfileChip({ profile }: Props) {
  if (!profile) return null;

  const tooltip =
    [
      profile.about,
      profile.contact ? `contact: ${profile.contact}` : null,
      profile.fill_latency_secs !== undefined
        ? `typical fill: ${profile.fill_latency_secs}s`
        : null,
      profile.min_cancel_delay !== undefined
        ? `min cancel delay: ${profile.min_cancel_delay}`
        : null,
      profile.pairs.length
        ? `pairs: ${profile.pairs
            .map((p) => `${p.backend}:${p.lock}/${p.want}`)
            .join(', ')}`
        : null,
    ]
      .filter(Boolean)
      .join('\n') || profile.name;

  return (
    <span className="inline-flex items-center gap-1 align-middle" title={tooltip}>
      {profile.fill_latency_secs !== undefined && (
        <span className="rounded-sm bg-ink-500/60 px-1 py-px text-[9px] font-mono uppercase tracking-wider text-zinc-400">
          ⏱ {profile.fill_latency_secs}s
        </span>
      )}
      {profile.badges.map((b) => (
        <span
          key={b}
          className="rounded-sm bg-mosaic-mint/15 px-1 py-px text-[9px] font-mono uppercase tracking-wider text-mosaic-mint ring-1 ring-mosaic-mint/30"
        >
          {b}
        </span>
      ))}
    </span>
  );
}
