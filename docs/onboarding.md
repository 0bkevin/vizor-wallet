# Mobile onboarding

This document describes the mobile onboarding behavior integrated in PR #793.
Desktop onboarding remains separate; this umbrella preserves its existing
entry paths. Mobile builds, tests, and captures use
`--dart-define=VIZOR_FORM_FACTOR=mobile`.

## Entry and account setup

| Entry | Account preparation | Completion |
| --- | --- | --- |
| Create a wallet | Introduction, address types, things to know, secret passphrase, passcode, account customisation | Optional Face ID, then Home |
| Import a secret passphrase | Phrase entry and review, wallet birthday, passcode when needed, account customisation | Optional Face ID for initial setup, then Home |
| Import a hardware wallet | Existing Keystone or Ledger connection, account/birthday steps, passcode when needed, account customisation | Existing capability and device gates apply |
| Link Vizor Desktop | Introduction, scan, account selection, contacts, passcode when needed | Optional Face ID for initial setup, then Home |
| Redeem a gift card into a new wallet | Card inspection, passcode, account customisation | Claim handoff, optional Face ID, then Home |
| Redeem a gift card into an imported wallet | Card inspection, existing import flow, receiving-account selection when needed | Claim handoff, optional Face ID, then Home |

- Welcome shows the gift card entry only before an account exists. Add-account
  entry omits it and reuses the configured passcode.
- A gift link opened without a wallet uses the same card entry screen. Existing
  wallets retain their unlock and gift card routes; other arriving cards remain
  queued while setup is in progress.
- Welcome uses the native video player with a static WebP poster underneath.
  The poster is also used for deterministic previews, reduced motion, or decoder
  failure. The mobile video includes a brief loop crossfade. Playback follows
  both route visibility and app lifecycle.
- Account customisation reuses the shared name/profile controls and random
  suggestions. Gift creation requires this step and has no Back or Skip action.
- The passcode is six digits, using the existing wallet credential model. The
  keypad's reset/help action appears only on the app-start unlock screen.
- Placeholder Terms/Privacy links are excluded until their documents exist.

## Progress bar

Progress describes the current position in **account preparation**. It does not
measure elapsed time, QR decoding, wallet sync, or gift claim completion.

`OnboardingProgressPlan` derives positions from semantic steps and an immutable
entry snapshot of whether a passcode must be created. The snapshot affects UI
only; authentication and account mutations still use the current security state.

Let `A = 60 / 196` (about 30.6%), `N` be the number of preparation steps, and `i`
be the step's position starting at 1:

```text
Entry position: A
Preparation step: A + (1 - A) × i / (N + 1)
Account ready: 1
```

| Flow | Preparation steps after the entry position |
| --- | --- |
| Create | Address types → Things to know → Secret passphrase → Passcode → Customise account |
| Passphrase import | Phrase entry → Phrase review → Birthday → Passcode → Customise account |
| Keystone | Device intro → Device scan → Account selection → Birthday → Passcode → Customise account |
| Ledger | Device connect → Birthday → Passcode → Customise account |
| Wallet Link | Link intro → Link scan → Account selection → Contact selection → Passcode |
| Gift creation | Passcode → Customise account |

- Welcome does not show a progress bar. Introduction and method/device selection
  use the common entry position.
- An existing passcode removes only the Passcode step from the selected plan.
  Account-ready/Face ID is 100%; the final preparation step stays below 100%.
- Back returns to that screen's position. Choosing another method starts that
  method's plan rather than carrying forward a previous maximum.
- Education Skip advances to Secret passphrase without removing the skipped
  steps. Paste/manual entry, passcode confirmation, retries, and modal sheets
  retain their current position.
- Progress is not persisted. The route context carries no duplicate credential
  or mnemonic data, and invalid flow/step combinations fail explicitly.

## Gift inspection, storage, and claim

### Before account creation

Explicit paste or scan inspects the card in a temporary claim wallet without a
receiving account. Checking uses the shared skeleton, hides amount/artwork, and
disables dismissal. Error and unavailable-card states expose their existing
exit/retry controls. Old birthdays keep the existing long-scan warning sheet.

Inspection is a snapshot. It does not guarantee that funds remain available or
that a later claim succeeds. A checked or confirmation-waiting card can proceed
to wallet setup.

### New-wallet commit boundary

1. Passcode confirmation retains the digits in live flow memory. It does not
   create the account or finish credential setup.
2. Customise Continue prepares the credential, creates the account, and durably
   saves its metadata and the incoming card with the receiving account UUID.
3. Commit the credential and finish the setup journal before handing the checked
   inspection to `PaymentLinkClaimCoordinator.claimSetupCard`.
4. Continue to optional Face ID and Home without waiting for binding, broadcast,
   or confirmations. The coordinator owns that work independently of the screen.

Pre-account failures remain inline and can roll back the newly prepared
credential. If account creation may already have succeeded, retain the credential
and recover the same account. A known UUID with incomplete storage is recovered
immediately; repeated failures lock the persona controls and offer **Try again**
on Customise. Retrying does not create another account or prepare its credential
again. An uncertain database result uses the existing reopen message and disables
recreation.

The live flow retains the inspection/passcode through route refresh and clears
them on completion or exit. They are not serialized into route restoration or
browser history. Locking routes to unlock; the durable journal supports recovery.

### Import and receiving-account selection

Choosing an existing wallet saves the card and the pre-import account UUIDs in
OS secure storage before leaving the card screen. All existing import methods
remain available: secret passphrase, Link Vizor Desktop, and the hardware wallet
options under their existing capability gates.

A sole imported account receives the card automatically. Multiple imported
accounts reuse **Choose receiving account**, including additional ZIP32 accounts.
Confirmation switches Home to that account, saves the pinned recipient, registers
the existing inspection, and clears the import journal before Face ID.

Closing the sheet continues to Face ID/Home with an unbound, unclaimed card in
**Settings > My gift cards > Received**. Restart before selection also preserves
it for manual claim. Recovery never guesses the recipient from the active
account, and a receiving choice already saved survives interruption during
journal cleanup.

### Execution and recovery

- Binding reuses the inspected database and re-estimates amount/fee for the
  saved recipient. **Do not add another sync or a background freshness scan at
  this handoff.** Existing later claim/recovery scans remain unchanged.
- Confirmation waiting remains pending. Definitive rejection/failure stays
  actionable in Received. An uncertain submission retains recovery state rather
  than being treated as a definite failure.
- Eligible saved setup claims can resume after restart, unlock, foreground
  entry, or the existing retry timer, after setup journals/start markers are
  cleared. Account switching cannot redirect them to another recipient.
- Failed, rejected, spent-elsewhere, archived, and terminal empty cards do not
  automatically submit. A missing receiving account is not substituted.
- Submitted claims use the existing Home Activity transaction identity. Waiting
  or failure without a txid does not create a synthetic transaction row.
- A definitive setup-claim failure shows a dismissible Home toast with
  **View card**, once the recipient's Home is visible. Face ID defers the notice;
  dismissal, leaving Home, locking, or account switching closes it. There is no
  Gift status banner, return page, or setup-failure screen.
- Received lists saved incoming cards, including pending/unsuccessful ones; it
  does not imply an on-chain receipt. Inspection/binding alone does not save a
  card, so setup must persist it before claim handoff.
- Destructive operations drain accepted setup/claim work before deleting account
  data. Successful claims clean up their temporary claim wallet and recovery
  secrets after the existing six-scanned-confirmation condition.

## Home, backup, and education

Home's setup carousel is **manual**, using swipe and page indicators. It contains
account-scoped backup guidance and Zcash education, with no chevron or automatic
rotation. Account customisation is completed before Home; neither carousel item
asks the user to name the account again.

The backup entry opens an introduction before revealing the phrase. **Remind me
later** hides that account's Home reminder without marking backup complete. Its
stored delays are 2 days, then 14 days, then 30 days for later deferrals; the
button does not promise a fixed number of days. Foreground entry and the deadline
refresh visibility. Settings still provides access during deferral.

Passcode or available biometric re-confirmation gates the reveal. Wallet birthday
height and best-effort date load alongside the phrase and have separate copy
actions when available. **I’ve written it down** saves explicit backup completion,
clears reminder deferral, and returns after persistence. The account remains
accessible in Settings even after Home guidance disappears. These reminder and
completion controls belong to post-creation backup, not the ordinary creation
screen's phrase/Continue step.

Zcash introduction, address types, and things-to-know screens complete education
independently of backup. Removing an unbacked account shows its backup warning;
removing the last account becomes **Reset Vizor**. Combined removal warnings
remain scrollable at enlarged text sizes and use button confirmation without
requiring typed `remove`.

## Preview and verification

Widgetbook: **Screens > Gift Cards > Mobile > Onboarding - Full walkthrough**.
Focused cases cover entry, checking, inspected card, passcode, customisation,
Face ID, long-scan warning, storage recovery, failure toast, and imported
receiving-account selection. Preview broadcast/confirmation adapters are
simulated; they are not evidence of a real transfer.

Deterministic capture scenarios include `mobile-gift-onboarding-entry`,
`mobile-gift-onboarding-checking`, `mobile-gift-onboarding-inspected`, and
`mobile-gift-onboarding-customise`. Use the widget renderer for content comparison:

```bash
scripts/figma-compare.sh widget --form-factor mobile --scenario <scenario> --theme <light|dark>
fvm flutter test --tags mobile --run-skipped --dart-define=VIZOR_FORM_FACTOR=mobile
fvm flutter test
fvm flutter analyze
```

Integrated validation on 2026-10-02 used an iPhone 17 Pro / iOS 26.3 simulator
and an isolated Zcash regtest chain. It covered native Welcome playback, a real
0.1 TAZ claim, Home balance/Activity, manual carousel, reminder deferral and
completion, birthday, education, Received, six scanned confirmations, and
ordinary creation/additional import. The walkthrough masked all 24 mnemonic
words before rendering; capture-only source changes and harnesses were removed.

A subsequent native run exercised **Accounts > Reset Vizor → create a wallet →
import an additional account**, without replacing the app root or directly
clearing storage after entry. It verified account/passcode reset, two accounts
after recreation/import, and no widget exceptions. This exposed and fixed an
unsupported Ledger outbox read during removal checks on regtest: recovery now
uses the existing Ledger capability gate, while mainnet lookup errors remain
errors. Focused regression tests cover both non-mainnet networks and mainnet.

An earlier combined harness that replaced app roots and directly reset storage
failed with an unmounted widget reference and a covered import-route payload
error. The supported UI sequence above did not reproduce those errors; their
exact cause in that artificial harness remains unconfirmed.

The recording used **Not now** on Face ID. Physical device/hardware pairing,
biometric enrollment/authentication, OS process termination of the claimed
wallet, native screenshot exclusion, and the complete repository test suite
remain unverified by this walkthrough. Native storage reload/unlock is not an
OS process-restart test.

Known follow-up: the deferred failure toast is in memory. Restart before Home
can lose the notice while the card remains durably recoverable in Received.
Persisting unseen notices is separate from the completed flow connection.

## Implementation references

- [Mobile routes and immutable progress context](../lib/src/core/navigation/mobile_onboarding_routes.dart)
- [Progress model](../lib/src/features/onboarding/mobile/mobile_onboarding_progress.dart)
- [Welcome media lifecycle](../lib/src/features/onboarding/shared/welcome_video_backdrop.dart)
- [Gift setup and handoff](../lib/src/features/payment_links/services/gift_claim_setup_coordinator.dart)
- [Credential/account setup boundary](../lib/src/features/payment_links/services/gift_wallet_setup.dart)
- [Claim coordinator](../lib/src/features/payment_links/providers/payment_link_claim_coordinator_provider.dart)
- [Post-creation backup](../lib/src/features/settings/screens/mobile/mobile_seed_phrase_screen.dart)
- [Home reminder visibility](../lib/src/features/home/providers/backup_reminder_provider.dart)
- [Account metadata and removal](../lib/src/providers/account_provider.dart)
