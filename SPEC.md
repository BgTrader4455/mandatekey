# MandateKey Mandate Specification — v0.1 (draft)

**Status:** Draft, open for comment
**License:** Apache-2.0
**Author:** Aleksandr Neykov

## 1. Purpose

AI agents increasingly hold wallets and pay on their own. Today every team invents its own spending limits, or ships none. This document defines an **agent mandate**: a small, open, on-chain primitive on Solana that lets an owner give an agent the right to spend funds **within enforceable limits**, and take that right back at any moment.

The goal is a shared baseline any agent framework, wallet or payment protocol (including x402) can adopt.

### 1.1 Design goals

1. **Enforced on-chain, not by convention.** An agent's key alone can never move more than the mandate allows.
2. **Owner stays in control.** The owner can revoke instantly and withdraw funds at any time, including after revocation.
3. **Least privilege by default.** No limit means no permission. Allowlists are deny-by-default.
4. **No token approvals.** Funds are never exposed through SPL `approve` / delegate authority (see §8.1).
5. **Co-signer neutral.** An optional external co-signer can be required above a threshold. The spec does not depend on any specific co-signing service.
6. **Small and auditable.** v0.1 covers SPL token transfers only. Arbitrary program calls are deferred (§10).

### 1.2 Non-goals for v0.1

- Arbitrary CPI into other programs (planned for v0.2).
- Native SOL spending (agents pay in SPL tokens / Token-2022; SOL support planned).
- Rolling-window accounting (v0.1 uses fixed windows).
- Judging whether an agent's purchase is *wise*. The mandate enforces bounds, not intent.

## 2. Terminology

| Term | Meaning |
|---|---|
| **Owner** | The human or organisation who funds the mandate and controls it. Root of authority. |
| **Agent** | The key used by an AI agent to request payments. |
| **Mandate** | An on-chain account holding the rules that bind an agent. |
| **Vault** | Token accounts owned by the mandate PDA, holding the funds the agent may spend. |
| **Co-signer** | An optional key whose signature is required for payments at or above a threshold. |
| **Period** | A fixed time window over which spending is capped. |

The key words MUST, SHOULD and MAY are used as in RFC 2119.

## 3. Accounts

### 3.1 Mandate account

PDA seeds: `["mandate", owner, agent, mandate_id (u64, LE)]`

| Field | Type | Description |
|---|---|---|
| `version` | `u8` | Spec version. `1` for v0.1. |
| `owner` | `Pubkey` | Owner key. |
| `agent` | `Pubkey` | Agent key. |
| `mandate_id` | `u64` | Lets one owner issue several mandates to one agent. |
| `cosigner` | `Option<Pubkey>` | Optional co-signer. |
| `valid_from` | `i64` | Unix timestamp. Payments before it MUST fail. |
| `expires_at` | `i64` | Unix timestamp. Payments at or after it MUST fail. |
| `revoked` | `bool` | Once `true`, never returns to `false`. |
| `intent_hash` | `[u8; 32]` | SHA-256 of the off-chain mandate document (§6). All zeros if unused. |
| `token_limits` | `Vec<TokenLimit>` | Max 8 entries in v0.1. |
| `destination_allowlist` | `Vec<Pubkey>` | Allowed recipient **wallet owners**. Max 32. Empty = any recipient allowed only if `allow_any_destination` is set. |
| `allow_any_destination` | `bool` | Default `false`. |
| `payment_count` | `u64` | Monotonic counter, used in events and for replay-safe client accounting. |
| `bump` | `u8` | PDA bump. |

### 3.2 TokenLimit

| Field | Type | Description |
|---|---|---|
| `mint` | `Pubkey` | Token mint. A mint not listed here cannot be spent. |
| `per_payment_max` | `u64` | Max amount in one payment (base units). |
| `period_max` | `u64` | Max total spent in one period. |
| `period_secs` | `u32` | Period length, e.g. `86400` for daily. MUST be > 0. |
| `period_start` | `i64` | Start of the current period. |
| `spent_in_period` | `u64` | Spent so far in the current period. |
| `cosign_threshold` | `u64` | Payments `>=` this need the co-signer. `u64::MAX` = never. |

### 3.3 Vault

For each `mint` in `token_limits`, the vault is the associated token account of the mandate PDA. Anyone MAY deposit into it. Only the program can move funds out, and only through the instructions in §4.

## 4. Instructions

### 4.1 `create_mandate` — signer: owner

Creates the mandate and vault accounts. MUST fail if `expires_at <= valid_from`, if any `period_secs == 0`, if any `per_payment_max > period_max`, or if list sizes exceed limits.

### 4.2 `pay` — signer: agent (and co-signer when required)

Transfers `amount` of `mint` from the vault to a recipient token account. The program MUST check, in this order, and fail on the first violation:

1. `revoked == false`
2. `valid_from <= now < expires_at`
3. `mint` is in `token_limits`
4. Recipient token account's owner is in `destination_allowlist`, or `allow_any_destination == true`
5. Period roll-over: if `now >= period_start + period_secs`, set `period_start` to the start of the window containing `now` and `spent_in_period = 0`
6. `0 < amount <= per_payment_max`
7. `spent_in_period + amount <= period_max` (checked arithmetic)
8. If `amount >= cosign_threshold`: `cosigner` is set **and** has signed the transaction
9. Vault balance `>= amount`

On success: update `spent_in_period`, increment `payment_count`, perform the transfer (`transfer_checked`), emit `PaymentMade`.

An optional `memo` (max 64 bytes) MAY be passed, e.g. an x402 payment reference. It is emitted in the event, not stored.

### 4.3 `update_mandate` — signer: owner

May tighten or loosen limits, allowlists, co-signer and expiry. MUST NOT reset `spent_in_period` for the current period unless the `mint` entry is removed and re-added. MUST fail if `revoked == true`.

### 4.4 `revoke` — signer: owner

Sets `revoked = true` immediately and permanently. Takes effect for every transaction processed after it. Emits `MandateRevoked`.

### 4.5 `withdraw` — signer: owner

Moves any amount from the vault to an owner-chosen account. Works in every state, including revoked or expired. The agent can never call it.

### 4.6 `close_mandate` — signer: owner

Requires all vault balances to be zero. Closes vault and mandate accounts and returns rent to the owner.

## 5. Events

| Event | Fields |
|---|---|
| `MandateCreated` | mandate, owner, agent, expires_at, intent_hash |
| `PaymentMade` | mandate, mint, amount, recipient, payment_count, spent_in_period, cosigned, memo |
| `MandateUpdated` | mandate, fields changed |
| `MandateRevoked` | mandate, slot |
| `Withdrawn` | mandate, mint, amount, destination |

## 6. Off-chain mandate document

The owner MAY describe the mandate in a human-readable JSON document. Its canonical form (RFC 8785, JSON Canonicalization Scheme) is hashed with SHA-256 and stored as `intent_hash`, so anyone can check that the on-chain rules match what the owner agreed to.

```json
{
  "spec": "mandatekey/0.1",
  "owner": "<base58>",
  "agent": "<base58>",
  "purpose": "Pay for API calls and datasets for research tasks",
  "valid_from": "2026-11-01T00:00:00Z",
  "expires_at": "2026-12-01T00:00:00Z",
  "limits": [
    {
      "mint": "<USDC mint>",
      "per_payment_max": "5.00",
      "period": "P1D",
      "period_max": "50.00",
      "cosign_at_or_above": "20.00"
    }
  ],
  "destinations": ["<base58>", "<base58>"],
  "cosigner": "<base58 or null>"
}
```

The on-chain program does not read this document. It is a commitment, not an enforcement mechanism.

## 7. Client integrations

These are SDK-level behaviours, specified so that different implementations agree.

### 7.1 SDK (TypeScript, Python)

- MUST read the mandate account and pre-check §4.2 rules before building a transaction, and return a typed error instead of submitting a transaction that will fail.
- SHOULD simulate the transaction before sending.

### 7.2 MCP server

- Exposes tools such as `get_mandate`, `get_remaining_budget` and `pay`.
- The agent never receives the owner key. The MCP server holds only the agent key.

### 7.3 x402 payment policy

- An x402 client using MandateKey MUST pay through `pay` from the vault, never through a token approval.
- MUST refuse a payment requirement whose amount, mint or `payTo` violates the mandate, before signing anything.
- SHOULD put the x402 payment reference in `memo`.

## 8. Security considerations

### 8.1 Why no token approvals

SPL `approve` gives a delegate a standing allowance that the token program enforces without any expiry, destination or period rules. A facilitator or client that treats an approval as a completed payment, or a leaked delegate key, can drain up to the full allowance. MandateKey keeps funds in a program-owned vault and moves them only through `pay`, so every movement passes every check.

### 8.2 Agent key compromise

A stolen agent key can spend at most the remaining `period_max` per mint, only to allowed destinations, only until expiry or revocation. Payments at or above `cosign_threshold` also need the co-signer. Owners SHOULD set limits assuming the agent key will eventually leak.

### 8.3 Owner key

The owner key is the root of trust. It SHOULD be a hardware wallet or a multisig (e.g. a Squads vault). Loss of the owner key means funds can be spent by the agent until expiry and cannot be withdrawn.

### 8.4 Clock

Time checks use the `Clock` sysvar, which can drift from wall-clock time by seconds to minutes. Owners SHOULD NOT rely on precision finer than a few minutes for `valid_from`, `expires_at` or period boundaries.

### 8.5 Revocation race

A `pay` transaction landing in an earlier slot than `revoke` will succeed. Revocation is final from the slot it lands in, not from the moment the owner clicks.

### 8.6 Destination checks

The allowlist is checked against the **owner of the recipient token account**, not the token account address, so an allowed recipient can receive into any of their token accounts for that mint. The recipient token account's mint MUST equal `mint`.

### 8.7 Token-2022 extensions

v0.1 MUST reject mints with transfer hooks, transfer fees or confidential transfers enabled, until their interaction with limits is specified.

## 9. Test suite

The reference implementation ships an open suite of attack scenarios that any implementation can run, including: overspend within a period, period boundary double-spend, payment after expiry, payment after revocation in the same block, unlisted mint, unlisted destination, recipient account with wrong mint, missing co-signer above threshold, co-signer signing alone, agent calling `withdraw`, arithmetic overflow on `spent_in_period`, and a malicious x402 payment requirement.

## 10. Open questions for v0.2

1. **Program allowlist / arbitrary CPI.** Letting agents call allowlisted programs (swaps, payments) while bounding the vault's net outflow per call.
2. **Native SOL** spending.
3. **Rolling windows** instead of fixed periods.
4. **Multiple agents per mandate** with a shared budget.
5. **Compatibility layer with Squads spending limits**, so a Squads vault can be the owner and funding source.

Comments and pull requests are welcome.

## Changelog

- **v0.1** — First public draft.
