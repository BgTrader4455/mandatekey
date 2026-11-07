# Attack test suite (planned, SPEC §9)

Each scenario MUST fail with the listed error. Status: [ ] not implemented yet.

| # | Scenario | Expected error |
|---|---|---|
| 1 | Pay more than `per_payment_max` | PerPaymentExceeded |
| 2 | Several payments exceeding `period_max` in one period | PeriodExceeded |
| 3 | Double-spend across a period boundary | PeriodExceeded / correct reset |
| 4 | Pay before `valid_from` | OutsideWindow |
| 5 | Pay at or after `expires_at` | OutsideWindow |
| 6 | Pay after `revoke` | Revoked |
| 7 | Pay with an unlisted mint | MintNotAllowed |
| 8 | Pay to an unlisted destination | DestinationNotAllowed |
| 9 | Recipient token account with a different mint | constraint violation |
| 10 | Amount above `cosign_threshold` without co-signer | CosignerRequired |
| 11 | Wrong key signing as co-signer | CosignerRequired |
| 12 | Co-signer signing alone, without agent | missing signature |
| 13 | Non-agent key calling `pay` | has_one violation |
| 14 | Agent calling `revoke` / `withdraw` | has_one violation |
| 15 | `spent_in_period` overflow | Overflow |
| 16 | Malicious x402 payment requirement (wrong `payTo` / mint) | refused by client policy |
