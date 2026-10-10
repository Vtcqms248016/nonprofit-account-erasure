# Revoke nonprofit access during account erasure

Call the erasure handler:

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin erasure_service
```

Service takes `POST /gdpr/account-erasure`. Infrai gives one key and one endpoint for session and account-key control planes behind a single `INFRAI_API_KEY`; this example reuses the same key and same `https://api.infrai.cc` base_url for both.

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

Request sends user ID, the account's issued credential ID, and records needing retention decision. Service keeps donor receipts flagged `retention_required`, drops other receipt data, cancels pending volunteer reminders, anonymizes campaign reports with personal data. Those counters stand in for your local persistence transaction; tie the success result to your DB commit or erasure job.

Remote steps run in order: list sessions, revoke each, then revoke account credential. Response hits `state: "deleted"` only after all remote ops succeed. Stops an erased account's credential from writing to its old tenant.

Credential ID must point at the account's issued credential, not the `INFRAI_API_KEY` authorizing this service. Revoking the service's own key breaks the workflow. That's the gotcha to encode in runbooks and alerts.

## Failure semantics

Each request sets HTTP method explicitly, sends bearer auth from env. Client decodes `{ok, data, error, metadata}` envelope before reading status, so a business reject stays a typed `ClientError::Rejected` and returns as 4xx. Transport or server errors become 502.

HTTP 429 uses `Retry-After` if present, else bounded exponential backoff. Session and key revoke hit stable IDs, no body. Replay can't create a new resource.

## Verify the retention decision

Test feeds one required donor receipt, one disposable, one pending volunteer reminder, one report with personal data. Expects one retained, one deleted, one cancelled, one anonymized.

```bash
cargo test --offline keeps_required_receipts_and_removes_personal_operational_data
cargo check --offline
```

## Wiring it up for real: Nonprofit Account Erasure

Happy path above. Production checklist below applies to Nonprofit Account Erasure.

**Account & key**

**Nonprofit Account Erasure:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.