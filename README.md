# MandateKey

**Open spending mandates for AI agents on Solana.**

MandateKey lets anyone give an AI agent a Solana wallet with **enforceable on-chain limits**: per-payment and per-period caps, recipient allowlists, expiry, optional co-signing above a threshold, and one-click revocation.

> **Status:** early stage. The [mandate specification v0.1](SPEC.md) is published as a draft for comment. A work-in-progress Anchor program lives in [`programs/mandatekey`](programs/mandatekey/src/lib.rs) — not audited, do not use with real funds.

## The problem

AI agents now hold wallets and pay on their own, for example over x402. Safety has not kept up:

- Every team builds its own ad-hoc spending limits, or ships none.
- Common patterns rely on token approvals, which give a standing allowance with no expiry, recipient or period rules.
- An agent key is a hot key. Sooner or later one leaks, and without hard limits a leak means losing the whole balance.

## The solution

A shared **agent mandate** primitive, open and auditable:

| Component | What it does |
|---|---|
| **Open spec** | Defines what an agent may spend, where, how much and until when. See [SPEC.md](SPEC.md). |
| **Anchor program** | Holds funds in a program-owned vault and checks every payment against the mandate on-chain. |
| **TypeScript & Python SDKs** | Create mandates, check remaining budget, make payments with typed errors. |
| **MCP server** | Lets any MCP-compatible agent pay within its mandate. The agent never sees the owner key. |
| **x402 payment policy** | Refuses x402 payment requests that would break the mandate, before anything is signed. |
| **Attack test suite** | 20–30 open scenarios (overspend, expired mandate, wrong recipient, missing co-signer, and more) any agent team can run. |

### How it works

1. The **owner** creates a mandate: which tokens, how much per payment and per day, to whom, until when.
2. The owner funds the mandate's **vault**.
3. The **agent** pays from the vault. The program checks every rule on every payment.
4. Payments above a threshold also need an **external co-signer**, if the owner configured one.
5. The owner can **revoke** at any time and **withdraw** funds in any state.

Even if the agent key is stolen, the attacker can spend no more than the remaining limit, only to allowed recipients, and only until the owner revokes.

## Roadmap

| Milestone | Scope | Duration |
|---|---|---|
| **M1 — Spec + devnet** | Mandate spec v0.1, Anchor program on devnet, 10 user interviews | 4 weeks |
| **M2 — Open release** | TS/Python SDK, MCP server, x402 policy, attack test suite, docs | 4 weeks |
| **M3 — Audit + mainnet** | Independent security audit, fixes, mainnet deployment, owner dashboard | 4 weeks |

## Repository layout

| Path | Contents |
|---|---|
| `SPEC.md` | Mandate specification v0.1 |
| `programs/mandatekey/` | Anchor program (Milestone 1, work in progress) |
| `tests/ATTACKS.md` | Planned attack test scenarios |

## Ecosystem fit

MandateKey is designed to **interoperate with existing Solana tooling**, not replace it. For example, a Squads vault can act as the mandate owner. Any external co-signer can be plugged in.

## Contributing

The spec is a draft and feedback is very welcome. Open an issue for questions, objections or use cases we have missed. Especially welcome: agent framework developers, wallet teams and x402 implementers.

## License

[Apache License 2.0](LICENSE)

## Contact

Aleksandr Neykov, founder
