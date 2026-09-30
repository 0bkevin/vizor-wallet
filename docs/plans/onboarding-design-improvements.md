# Onboarding design improvements

## Scope

Integrate the onboarding designs through small pull requests targeting
`rowan/onboarding-design-integration`. Start from the current upstream base;
extract UI changes without importing the Gift Card feature branch history.

## Delivery slices

1. Mobile and desktop Welcome: video/poster assets, brief loop crossfades,
   adjusted gradient overlays, button tokens and interaction effects.
   Show the Gift Card button only before an account exists; keep redemption
   disconnected with a source TODO until the claim flow is finalized.
2. Mobile introduction: card, pattern, spacing, progress track, and actions.
3. Desktop introduction: text wrapping and sidebar spacing.
4. Mobile import method selection.
5. Mobile hardware wallet selection.
6. Desktop import method selection.
7. Desktop hardware wallet selection.

Desktop import and hardware selectors form a prerequisite PR for Welcome so
Keystone and Ledger remain reachable after the old Welcome actions are replaced.
Mobile Welcome keeps its existing create/import destinations until the mobile
selector slice is integrated. Each slice includes its focused tests and
deterministic Figma captures.

## Deferred behavior

- Gift Card claim, account setup, recovery, and incoming-link routing.
- Gift Card progress and education banners on Home.
- Terms/Privacy footer links until the actual documents are available.
- Link Vizor Desktop on desktop, which has no supported entry flow.

## References

- [Mobile Welcome](https://www.figma.com/design/jhozt3bbbVYms9MkpGJgoI/Vizor--Design-System?node-id=8635-103040)
- [Desktop Welcome and selectors](https://www.figma.com/design/jhozt3bbbVYms9MkpGJgoI/Vizor--Design-System?node-id=8648-103130)
- [Accent hover](https://www.figma.com/design/jhozt3bbbVYms9MkpGJgoI/Vizor--Design-System?node-id=8668-27441)
- [Secondary hover](https://www.figma.com/design/jhozt3bbbVYms9MkpGJgoI/Vizor--Design-System?node-id=8668-27447)
