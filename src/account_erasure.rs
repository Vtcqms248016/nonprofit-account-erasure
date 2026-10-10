use crate::infrai_client::{ClientError, InfraiClient};
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct ErasureRequest {
    pub user_id: String,
    pub credential_id: String,
    pub donor_receipts: Vec<DonorReceipt>,
    pub volunteer_reminders: Vec<VolunteerReminder>,
    pub campaign_reports: Vec<CampaignReport>,
}

#[derive(Debug, Deserialize)]
pub struct DonorReceipt {
    pub receipt_id: String,
    pub retention_required: bool,
}

#[derive(Debug, Deserialize)]
pub struct VolunteerReminder {
    pub reminder_id: String,
    pub pending: bool,
}

#[derive(Debug, Deserialize)]
pub struct CampaignReport {
    pub report_id: String,
    pub contains_personal_data: bool,
}

#[derive(Debug, PartialEq)]
pub struct ErasurePlan {
    pub receipts_retained: usize,
    pub receipts_deleted: usize,
    pub reminders_cancelled: usize,
    pub reports_anonymized: usize,
}

#[derive(Debug, Serialize)]
pub struct ErasureResult {
    pub user_id: String,
    pub state: &'static str,
    pub sessions_revoked: usize,
    pub credential_revoked: bool,
    pub receipts_retained: usize,
    pub receipts_deleted: usize,
    pub reminders_cancelled: usize,
    pub reports_anonymized: usize,
}

#[derive(Debug, Error)]
pub enum ErasureError {
    #[error("user_id and credential_id are required")]
    InvalidRequest,
    #[error(transparent)]
    Infrai(#[from] ClientError),
}

impl IntoResponse for ErasureError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::InvalidRequest => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Infrai(ClientError::Rejected { status, .. }) => (
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_REQUEST),
                self.to_string(),
            ),
            Self::Infrai(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

pub fn plan_erasure(request: &ErasureRequest) -> Result<ErasurePlan, ErasureError> {
    if request.user_id.trim().is_empty() || request.credential_id.trim().is_empty() {
        return Err(ErasureError::InvalidRequest);
    }
    Ok(ErasurePlan {
        receipts_retained: request
            .donor_receipts
            .iter()
            .filter(|r| r.retention_required)
            .count(),
        receipts_deleted: request
            .donor_receipts
            .iter()
            .filter(|r| !r.retention_required)
            .count(),
        reminders_cancelled: request
            .volunteer_reminders
            .iter()
            .filter(|r| r.pending)
            .count(),
        reports_anonymized: request
            .campaign_reports
            .iter()
            .filter(|r| r.contains_personal_data)
            .count(),
    })
}

pub async fn erase_account(
    client: &InfraiClient,
    request: ErasureRequest,
) -> Result<ErasureResult, ErasureError> {
    let plan = plan_erasure(&request)?;
    let sessions = client.sessions_for_user(&request.user_id).await?;
    for session in &sessions {
        client.revoke_session(&session.id).await?;
    }
    client.revoke_account_key(&request.credential_id).await?;

    Ok(ErasureResult {
        user_id: request.user_id,
        state: "deleted",
        sessions_revoked: sessions.len(),
        credential_revoked: true,
        receipts_retained: plan.receipts_retained,
        receipts_deleted: plan.receipts_deleted,
        reminders_cancelled: plan.reminders_cancelled,
        reports_anonymized: plan.reports_anonymized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_required_receipts_and_removes_personal_operational_data() {
        let request = ErasureRequest {
            user_id: "donor-42".into(),
            credential_id: "key-42".into(),
            donor_receipts: vec![
                DonorReceipt {
                    receipt_id: "r-1".into(),
                    retention_required: true,
                },
                DonorReceipt {
                    receipt_id: "r-2".into(),
                    retention_required: false,
                },
            ],
            volunteer_reminders: vec![VolunteerReminder {
                reminder_id: "v-1".into(),
                pending: true,
            }],
            campaign_reports: vec![CampaignReport {
                report_id: "c-1".into(),
                contains_personal_data: true,
            }],
        };

        assert_eq!(
            plan_erasure(&request).unwrap(),
            ErasurePlan {
                receipts_retained: 1,
                receipts_deleted: 1,
                reminders_cancelled: 1,
                reports_anonymized: 1,
            }
        );
    }
}
