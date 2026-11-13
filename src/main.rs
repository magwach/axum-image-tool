mod auth;
mod db;
mod error;
mod handlers;
mod processing;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024; // 20 MB

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_token: String,
}

fn app(state: AppState) -> Router {
    Router::new()
        // public
        .route("/", get(handlers::index))
        .route("/pricing", get(handlers::pricing))
        // billed endpoint (requires x-api-key)
        .route("/process", post(handlers::process_handler))
        // account endpoints (require x-api-key)
        .route("/billing/balance", get(handlers::balance))
        .route("/billing/usage", get(handlers::usage))
        // admin endpoints (require x-admin-token)
        .route("/admin/accounts", post(handlers::create_account))
        .route("/admin/accounts/:id/credit", post(handlers::credit_account))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    // Load variables from a local .env file if present (ignored in production).
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let admin_token = std::env::var("ADMIN_TOKEN").expect("ADMIN_TOKEN must be set");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to connect to Postgres");

    // Applies any new SQL files from ./migrations on startup.
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("failed to run database migrations");

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind port");
    println!("Listening on port {port}");

    axum::serve(listener, app(AppState { pool, admin_token }))
        .await
        .expect("server error");
}