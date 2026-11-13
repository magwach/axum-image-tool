use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub balance_kes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct UsageRecord {
    pub id: i64,
    pub operations: String,
    pub input_bytes: i64,
    pub output_bytes: i64,
    pub cost_kes: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn create_account(
    pool: &PgPool,
    name: &str,
    api_key_hash: &str,
    initial_balance_kes: i64,
) -> Result<Account, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        "INSERT INTO accounts (name, api_key_hash, balance_kes)
         VALUES ($1, $2, $3)
         RETURNING id, name, balance_kes, created_at",
    )
    .bind(name)
    .bind(api_key_hash)
    .bind(initial_balance_kes)
    .fetch_one(pool)
    .await
}

pub async fn find_account_by_key_hash(
    pool: &PgPool,
    api_key_hash: &str,
) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        "SELECT id, name, balance_kes, created_at
         FROM accounts
         WHERE api_key_hash = $1",
    )
    .bind(api_key_hash)
    .fetch_optional(pool)
    .await
}

pub async fn credit_account(
    pool: &PgPool,
    account_id: i64,
    amount_kes: i64,
) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        "UPDATE accounts
         SET balance_kes = balance_kes + $2
         WHERE id = $1
         RETURNING id, name, balance_kes, created_at",
    )
    .bind(account_id)
    .bind(amount_kes)
    .fetch_optional(pool)
    .await
}

pub async fn charge_request(
    pool: &PgPool,
    account_id: i64,
    cost_kes: i64,
    operations: &str,
    input_bytes: i64,
    output_bytes: i64,
) -> Result<Option<i64>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let new_balance: Option<i64> = sqlx::query_scalar(
        "UPDATE accounts
         SET balance_kes = balance_kes - $2
         WHERE id = $1 AND balance_kes >= $2
         RETURNING balance_kes",
    )
    .bind(account_id)
    .bind(cost_kes)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(balance) = new_balance else {
        tx.rollback().await?;
        return Ok(None);
    };

    sqlx::query(
        "INSERT INTO usage_records (account_id, operations, input_bytes, output_bytes, cost_kes)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(account_id)
    .bind(operations)
    .bind(input_bytes)
    .bind(output_bytes)
    .bind(cost_kes)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(balance))
}

pub async fn recent_usage(
    pool: &PgPool,
    account_id: i64,
    limit: i64,
) -> Result<Vec<UsageRecord>, sqlx::Error> {
    sqlx::query_as::<_, UsageRecord>(
        "SELECT id, operations, input_bytes, output_bytes, cost_kes, created_at
         FROM usage_records
         WHERE account_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2",
    )
    .bind(account_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn usage_totals(pool: &PgPool, account_id: i64) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*), COALESCE(SUM(cost_kes), 0)::BIGINT
         FROM usage_records
         WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await
}