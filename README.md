# axum-image-tool

An Axum web service that resizes, rotates and converts images to grayscale,
with per-request billing backed by Postgres.

Each customer is an **account** with an API key and a prepaid balance. Every
successful `/process` request is charged and recorded in a usage log.

## Setup

1. Start Postgres (any instance works; this is a quick local one):

   ```bash
   docker run --name imagetool-db -e POSTGRES_PASSWORD=password \
     -e POSTGRES_DB=imagetool -p 5432:5432 -d postgres:16
   ```

2. Copy `.env.example` to `.env` and adjust it:

   ```
   DATABASE_URL=postgres://postgres:password@localhost:5432/imagetool
   ADMIN_TOKEN=change-me-to-a-long-random-string
   PORT=3000
   ```

3. Run it. Migrations in `migrations/` are applied automatically on startup.

   ```bash
   cargo run --release
   ```

## Pricing

Prices are in whole Kenyan shillings (KES), stored as integers.

| Item             | Price (KES) |
| ---------------- | ----------- |
| Base per request | 1           |
| + resize         | 1           |
| + rotate         | 1           |
| + grayscale      | 1           |

Only successful requests are billed. See `GET /pricing`. Prices are constants
at the top of `src/processing.rs`.

## API

| Method | Path                         | Auth            | Purpose                         |
| ------ | ---------------------------- | --------------- | ------------------------------- |
| GET    | `/`                          | none            | Upload page                     |
| GET    | `/pricing`                   | none            | Current prices                  |
| POST   | `/process`                   | `x-api-key`     | Process an image (billed)       |
| GET    | `/billing/balance`           | `x-api-key`     | Current balance                 |
| GET    | `/billing/usage`             | `x-api-key`     | Totals and last 50 requests     |
| POST   | `/admin/accounts`            | `x-admin-token` | Create account, returns API key |
| POST   | `/admin/accounts/:id/credit` | `x-admin-token` | Add funds to an account         |

`POST /process` is multipart/form-data with fields `image` (required), `width`,
`height`, `rotate` (0/90/180/270), `grayscale` (true/on), `format` (png/jpeg).
The response includes `x-request-cost-kes` and `x-balance-kes` headers.
Insufficient balance returns `402 Payment Required`.

### Example flow

```bash
# 1. Create an account with KES 50 credit. Save the api_key: it is shown once.
curl -X POST http://127.0.0.1:3000/admin/accounts \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"name": "Acme", "initial_balance_kes": 50}'

# 2. Process an image
curl -X POST http://127.0.0.1:3000/process \
  -H "x-api-key: imgk_..." \
  -F image=@photo.png -F width=300 -F rotate=90 -F grayscale=true \
  -F format=jpeg -o out.jpg

# 3. Check balance and usage
curl http://127.0.0.1:3000/billing/balance -H "x-api-key: imgk_..."
curl http://127.0.0.1:3000/billing/usage   -H "x-api-key: imgk_..."

# 4. Top up account 1 with KES 100
curl -X POST http://127.0.0.1:3000/admin/accounts/1/credit \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"amount_kes": 100}'
```

## Deploying on Render

1. Create a **PostgreSQL** instance on Render.
2. On your web service, add environment variables:
   - `DATABASE_URL`: the database's connection URL (use the internal URL if
     both are in the same region)
   - `ADMIN_TOKEN`: a long random secret
3. Build command `cargo build --release`, start command
   `./target/release/axum-image-tool`.

## Tests

```bash
cargo test
```

The unit tests cover image processing, pricing and API key helpers. They don't
need a database.

## Layout

- `src/main.rs`: startup, DB pool, migrations, routes
- `src/handlers.rs`: HTTP handlers (process, billing, admin)
- `src/processing.rs`: image transforms, pricing, tests
- `src/db.rs`: Postgres queries
- `src/auth.rs`: API key generation and hashing
- `src/error.rs`: error type
- `src/index.html`: upload page
- `migrations/`: SQL schema
