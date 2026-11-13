use axum::{
    extract::{ Multipart, Path, State },
    http::{ header, HeaderMap, HeaderName },
    response::{ Html, IntoResponse, Response },
    Json,
};
use image::ImageFormat;
use serde::{ Deserialize, Serialize };

use crate::{
    auth,
    db::{ self, Account, UsageRecord },
    error::AppError,
    processing::{
        self,
        Options,
        BASE_PRICE_KES,
        GRAYSCALE_PRICE_KES,
        RESIZE_PRICE_KES,
        ROTATE_PRICE_KES,
    },
    AppState,
};

const MAX_DIMENSION: u32 = 8192;

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Account, AppError> {
    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("missing x-api-key header"))?;

    db::find_account_by_key_hash(&state.pool, &auth::hash_api_key(key)).await?.ok_or_else(||
        AppError::unauthorized("invalid API key")
    )
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let token = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    if token == Some(state.admin_token.as_str()) {
        Ok(())
    } else {
        Err(AppError::unauthorized("invalid admin token"))
    }
}

fn parse_dimension(name: &str, text: &str) -> Result<Option<u32>, AppError> {
    if text.is_empty() {
        return Ok(None);
    }
    let value: u32 = text
        .parse()
        .map_err(|_| AppError::bad_request(format!("{name} must be a positive integer")))?;
    if value == 0 || value > MAX_DIMENSION {
        return Err(AppError::bad_request(format!("{name} must be between 1 and {MAX_DIMENSION}")));
    }
    Ok(Some(value))
}

pub async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

#[derive(Serialize)]
pub struct Pricing {
    currency: &'static str,
    base_kes: i64,
    resize_kes: i64,
    rotate_kes: i64,
    grayscale_kes: i64,
}

pub async fn pricing() -> Json<Pricing> {
    Json(Pricing {
        currency: "KES",
        base_kes: BASE_PRICE_KES,
        resize_kes: RESIZE_PRICE_KES,
        rotate_kes: ROTATE_PRICE_KES,
        grayscale_kes: GRAYSCALE_PRICE_KES,
    })
}

pub async fn process_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart
) -> Result<Response, AppError> {
    let account = authenticate(&state, &headers).await?;

    let mut file: Option<Vec<u8>> = None;
    let mut opts = Options::default();

    while
        let Some(field) = multipart
            .next_field().await
            .map_err(|e| AppError::bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if name == "image" {
            let data = field.bytes().await.map_err(|e| AppError::bad_request(e.to_string()))?;
            if !data.is_empty() {
                file = Some(data.to_vec());
            }
            continue;
        }

        let text = field.text().await.map_err(|e| AppError::bad_request(e.to_string()))?;
        let text = text.trim();

        match name.as_str() {
            "width" => {
                opts.width = parse_dimension("width", text)?;
            }
            "height" => {
                opts.height = parse_dimension("height", text)?;
            }
            "rotate" if !text.is_empty() => {
                opts.rotate = text
                    .parse()
                    .map_err(|_| AppError::bad_request("rotate must be a number"))?;
            }
            "grayscale" => {
                opts.grayscale = matches!(
                    text.to_lowercase().as_str(),
                    "on" | "true" | "1" | "yes"
                );
            }
            "format" if !text.is_empty() => {
                opts.format = match text.to_lowercase().as_str() {
                    "png" => ImageFormat::Png,
                    "jpg" | "jpeg" => ImageFormat::Jpeg,
                    other => {
                        return Err(
                            AppError::bad_request(
                                format!("unsupported format '{other}' (use png or jpeg)")
                            )
                        );
                    }
                };
            }
            _ => {}
        }
    }

    let bytes = file.ok_or_else(|| AppError::bad_request("missing 'image' file field"))?;

    let cost = opts.cost_kes();
    if account.balance_kes < cost {
        return Err(
            AppError::payment_required(
                format!(
                    "insufficient balance: this request costs KES {cost} but your balance is KES {}",
                    account.balance_kes
                )
            )
        );
    }

    let operations = opts.operations();
    let format = opts.format;
    let input_len = bytes.len() as i64;

    let output = tokio::task
        ::spawn_blocking(move || processing::transform(&bytes, &opts)).await
        .map_err(|e| AppError::internal(e.to_string()))??;

    let new_balance = db
        ::charge_request(
            &state.pool,
            account.id,
            cost,
            &operations,
            input_len,
            output.len() as i64
        ).await?
        .ok_or_else(|| AppError::payment_required("insufficient balance"))?;

    Ok(
        (
            [
                (header::CONTENT_TYPE, format.to_mime_type().to_string()),
                (HeaderName::from_static("x-request-cost-kes"), cost.to_string()),
                (HeaderName::from_static("x-balance-kes"), new_balance.to_string()),
            ],
            output,
        ).into_response()
    )
}

#[derive(Serialize)]
pub struct BalanceResponse {
    account_id: i64,
    name: String,
    balance_kes: i64,
}

pub async fn balance(
    State(state): State<AppState>,
    headers: HeaderMap
) -> Result<Json<BalanceResponse>, AppError> {
    let account = authenticate(&state, &headers).await?;
    Ok(
        Json(BalanceResponse {
            account_id: account.id,
            name: account.name,
            balance_kes: account.balance_kes,
        })
    )
}

#[derive(Serialize)]
pub struct UsageResponse {
    total_requests: i64,
    total_spent_kes: i64,
    recent: Vec<UsageRecord>,
}

pub async fn usage(
    State(state): State<AppState>,
    headers: HeaderMap
) -> Result<Json<UsageResponse>, AppError> {
    let account = authenticate(&state, &headers).await?;
    let (total_requests, total_spent_kes) = db::usage_totals(&state.pool, account.id).await?;
    let recent = db::recent_usage(&state.pool, account.id, 50).await?;

    Ok(
        Json(UsageResponse {
            total_requests,
            total_spent_kes,
            recent,
        })
    )
}

#[derive(Deserialize)]
pub struct CreateAccount {
    name: String,
    #[serde(default)]
    initial_balance_kes: i64,
}

#[derive(Serialize)]
pub struct CreatedAccount {
    account: Account,
    api_key: String,
}

pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateAccount>
) -> Result<Json<CreatedAccount>, AppError> {
    require_admin(&state, &headers)?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("name must not be empty"));
    }
    if body.initial_balance_kes < 0 {
        return Err(AppError::bad_request("initial_balance_kes must not be negative"));
    }

    let api_key = auth::generate_api_key();
    let account = db::create_account(
        &state.pool,
        name,
        &auth::hash_api_key(&api_key),
        body.initial_balance_kes
    ).await?;

    Ok(Json(CreatedAccount { account, api_key }))
}

#[derive(Deserialize)]
pub struct Credit {
    amount_kes: i64,
}

pub async fn credit_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<Credit>
) -> Result<Json<Account>, AppError> {
    require_admin(&state, &headers)?;

    if body.amount_kes <= 0 {
        return Err(AppError::bad_request("amount_kes must be positive"));
    }

    db::credit_account(&state.pool, id, body.amount_kes).await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("account not found"))
}
