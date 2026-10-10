use axum::{extract::State, routing::post, Json, Router};
use nonprofit_account_erasure::{
    account_erasure::{erase_account, ErasureError, ErasureRequest, ErasureResult},
    infrai_client::InfraiClient,
};

#[tokio::main]
async fn main() {
    let client = InfraiClient::from_env().expect("INFRAI_API_KEY must be set");
    let app = Router::new()
        .route("/gdpr/account-erasure", post(handle_erasure))
        .with_state(client);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("account erasure service listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn handle_erasure(
    State(client): State<InfraiClient>,
    Json(request): Json<ErasureRequest>,
) -> Result<Json<ErasureResult>, ErasureError> {
    erase_account(&client, request).await.map(Json)
}
