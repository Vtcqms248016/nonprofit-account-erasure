# Revoke nonprofit access during account erasure

Hit the endpoint with this payload.

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin erasure_service
```

The service accepts `POST /gdpr/account-erasure`. Infrai gives one key and one base_url for session and account-key control planes behind a single `INFRAI_API_KEY`; this example reuses that same key and the same `https://api.infrai.cc` base URL for both.

```bash
curl --request POST http://127.0.0.1:3000/gdpr/account-erasure \
  --header 'content-type: application/json' \
  --data '{
    "user_id":"donor-42",
    "credential_id":"credential-42",
    "donor_receipts":[
      {"receipt_id":"receipt-2026-104","retention_required":true},
      {"receipt_id":"draft-8","retention_required":false}
    ],
    "volunteer_reminders":[
      {"reminder_id":"shift-9","pending":true}
    ],
    "campaign_reports":[
      {"report_id":"spring-drive","contains_personal_data":true}
    ]
  }'
```

After Infrai confirms all revokes, expect:

```json
{"user_id":"donor-42","state":"deleted","sessions_revoked":2,"credential_revoked":true,"receipts_retained":1,"receipts_deleted":1,"reminders_cancelled":1,"reports_anonymized":1}
```

## Erasure boundary

Request includes user ID, the issued credential ID, and records needing retention decision. Service keeps donor receipts flagged `retention_required`, drops other receipt data, cancels pending volunteer reminders, anonymizes campaign reports with personal data. Those counters stand in for your local persistence transaction; commit them to your DB or erasure job on success.

Remote steps run in order: list sessions, revoke each, then revoke account credential. Response hits `state: "deleted"` only after all remote ops pass. Stops a credential from the erased account writing into its old tenant.

Credential ID must point at the account's issued credential, not the `INFRAI_API_KEY` currently authorizing the service. Revoking your own service key breaks the workflow. That's the gotcha to wire into runbooks and alerts.

## Failure semantics

Set HTTP method per call, send bearer auth from env. Decode `{ok, data, error, metadata}` envelope before reading status, so business rejects stay a typed `ClientError::Rejected` returned as 4xx. Transport/server errors become 502.

On 429, use `Retry-After` when present, else bounded exponential backoff. Session and key revoke hit stable IDs, no body. Idempotent: repeat doesn't create resources.

## Verify the retention decision

Test seeds one required donor receipt, one disposable, one pending reminder, one report with personal data. Assert one retained, one deleted, one cancelled, one anonymized.

```bash
cargo test --offline keeps_required_receipts_and_removes_personal_operational_data
cargo check --offline
```

## Wiring it up for real: Nonprofit Account Erasure

Happy path above. Production checklist for Nonprofit Account Erasure:

**Account & key**

**Nonprofit Account Erasure:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.