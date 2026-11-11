# Private Signet preview

## Product scope

Mosaik is the order-book/taker interface; desktop/p2pd runs the maker, and the
extension holds wallet keys and approves the trade. Use Signet custom
(Mutinynet) only. Offers are filled in full. Maker uptime is required.
KaleidoSwap's route/quote web app remains a separate product; share Kaleido UI,
wallet integration and network terminology rather than merging settlement flows.
Covenant/Tessera is Liquid/Simplicity research in Labs, disabled by default.

## Build and verify

Node 22.12+ (22.23.1 verified), npm lockfile. No API keys in frontend env files.

```sh
npm ci
npm audit
npm test
npm run build -- --mode signet
```

`dist` contains static files and security headers. Cloudflare Pages serves SPA
fallback without a Functions backend. No Mosaik API or maker wallet is hosted
on Pages. Browser discovery uses `wss://relay.kaleidoswap.com`; the asset
registry uses `https://api.rgbmap.org`. All signing stays in the extension.

## Protect before upload

Account: KaleidoSwap Com Account (`79610a759ca5eb63ee475aee6deb06b0`).
Project: `mosaik-signet-preview`. Do not deploy until Access is configured.

1. Configure Cloudflare Access for BOTH `mosaik-signet-preview.pages.dev` and
   `*.mosaik-signet-preview.pages.dev`. Cover production, branch aliases and
   unique deployment URLs. A preview-only Pages switch does not protect root.
2. Allow only explicitly approved testers (initial owner: waltermaffy@gmail.com).
   Configure email OTP or the existing identity provider; no bypass/everyone rule.
3. Verify protection with an unauthenticated request on every applicable hostname.
4. Only then deploy `dist` with Wrangler to the protected project.
5. Confirm anonymous requests reach Access, owner login works, CSP permits the
   relay/registry and the extension connects on this origin. Verify Trade,
   Activity, Wallet, Guide, Labs and a disposable-wallet E2E on Mutinynet.

No automatic deployment workflow is enabled. CI only tests/builds and does not
upload artifacts. A custom domain can be added later with equivalent Access
protection, before public beta approval.

## Rollback and launch gates

Retain the prior successful deployment ID and roll back within Pages if a
regression occurs; retain Access protection throughout. Public launch remains
separate from deploying this restricted preview. Before launch: fresh hosted
E2E, wallet permission/recovery checks, maker availability, error telemetry
without wallet secrets, support documentation and an explicit tester rollout.

## Deployed private preview (11 November 2026)

- Stable preview: https://private.mosaik-signet-preview.pages.dev
- Deployment: https://f66c2b2d.mosaik-signet-preview.pages.dev
- Access application: `1fabf069-c5e7-494b-be5b-724a078fd430`.
- Email OTP only, allow `waltermaffy@gmail.com`, 12-hour sessions.
- Root, wildcard aliases and direct deployment URL are covered. Anonymous
  requests to both the document and JavaScript assets redirect to Access.
- Production branch `main` is undeployed. Only the `private` branch has Mosaik.
- Account ID is supplied through `CLOUDFLARE_ACCOUNT_ID`, not the Pages config.

Create the empty Pages project before the Access application: Cloudflare must
recognise ownership of its pages.dev hostname. Verify protection with a blank
placeholder deployment before uploading the app. Tokens stay outside the repo.

Owner login and hosted wallet integration are separate verification gates;
local wallet E2E results do not substitute for a hosted-origin test.
