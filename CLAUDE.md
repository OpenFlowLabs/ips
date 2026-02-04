# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Barycenter is an OpenID Connect Identity Provider (IdP) implementing OAuth 2.0 Authorization Code flow with PKCE. The project is written in Rust using axum for the web framework, SeaORM for database access (SQLite and PostgreSQL), and josekit for JOSE/JWT operations.

## Build and Development Commands

```bash
# Build the project
cargo build

# Run the application (defaults to config.toml)
cargo run

# Run with custom config
cargo run -- --config path/to/config.toml

# Run in release mode
cargo build --release
cargo run --release

# Check code without building
cargo check

# Run tests (IMPORTANT: use cargo nextest, not cargo test)
cargo nextest run

# Run with logging (uses RUST_LOG environment variable)
RUST_LOG=debug cargo run
RUST_LOG=barycenter=trace cargo run
```

## Testing

**CRITICAL: Always use `cargo nextest run` instead of `cargo test`.**

This project uses [cargo-nextest](https://nexte.st/) for running tests because:
- Tests run in separate processes, preventing port conflicts in integration tests
- Better test isolation and reliability
- Cleaner output and better performance

Install nextest if you don't have it:
```bash
cargo install cargo-nextest
```

Run tests:
```bash
# Run all tests
cargo nextest run

# Run with verbose output
cargo nextest run --verbose

# Run specific test
cargo nextest run test_name
```

## Configuration

The application loads configuration from:
1. Default values (defined in `src/settings.rs`)
2. Configuration file (default: `config.toml`)
3. Environment variables with prefix `BARYCENTER__` (e.g., `BARYCENTER__SERVER__PORT=9090`)

Environment variables use double underscores as separators for nested keys.

### Database Configuration

Barycenter supports both SQLite and PostgreSQL databases. The database backend is automatically detected from the connection string:

**SQLite (default):**
```toml
[database]
url = "sqlite://barycenter.db?mode=rwc"
```

**PostgreSQL:**
```toml
[database]
url = "postgresql://user:password@localhost/barycenter"
```

Or via environment variable:
```bash
export BARYCENTER__DATABASE__URL="postgresql://user:password@localhost/barycenter"
```

## Architecture and Module Structure

### Entry Point (`src/main.rs`)
The application initializes in this order:
1. Parse CLI arguments for config file path
2. Load settings from config file and environment
3. Initialize database connection and create tables via `storage::init()`
4. Initialize JWKS manager (generates or loads RSA keys)
5. Start web server with `web::serve()`

### Settings (`src/settings.rs`)
Manages configuration with four main sections:
- `Server`: listen address and public base URL (issuer)
- `Database`: database connection string (SQLite or PostgreSQL)
- `Keys`: JWKS and private key paths, signing algorithm
- `Federation`: trust anchor URLs (future use)

The `issuer()` method returns the OAuth issuer URL, preferring `public_base_url` or falling back to `http://{host}:{port}`.

### Storage (`src/storage.rs`)
Database layer with raw SQL using SeaORM's `DatabaseConnection`. Supports both SQLite and PostgreSQL backends, automatically detected from the connection string. Tables:
- `clients`: OAuth client registrations (client_id, client_secret, redirect_uris)
- `auth_codes`: Authorization codes with PKCE challenge, subject, scope, nonce
- `access_tokens`: Bearer tokens with subject, scope, expiration
- `properties`: Key-value store for arbitrary user properties (owner, key, value)

All IDs and tokens are generated via `random_id()` (24 random bytes, base64url-encoded).

### JWKS Manager (`src/jwks.rs`)
Handles RSA key generation, persistence, and JWT signing:
- Generates 2048-bit RSA key on first run
- Persists private key as JSON to `private_key_path`
- Publishes public key set to `jwks_path`
- Provides `sign_jwt_rs256()` for ID Token signing with kid header

### Web Endpoints (`src/web.rs`)
Implements OpenID Connect and OAuth 2.0 endpoints:

**Discovery & Registration:**
- `GET /.well-known/openid-configuration` - OpenID Provider metadata
- `GET /.well-known/jwks.json` - Public signing keys
- `POST /connect/register` - Dynamic client registration

**OAuth/OIDC Flow:**
- `GET /authorize` - Authorization endpoint (issues authorization code with PKCE)
  - Validates client_id, redirect_uri, scope (must include "openid"), PKCE S256
  - Checks 2FA requirements (admin-enforced, high-value scopes, max_age)
  - Redirects to /login or /login/2fa if authentication needed
  - Returns redirect with code and state
- `POST /token` - Token endpoint (exchanges code for tokens)
  - Supports `client_secret_basic` (Authorization header) and `client_secret_post` (form body)
  - Validates PKCE S256 code_verifier
  - Returns access_token, id_token (JWT with AMR/ACR claims), token_type, expires_in
- `GET /userinfo` - UserInfo endpoint (returns claims for Bearer token)

**Authentication:**
- `GET /login` - Login page with passkey autofill and password fallback
- `POST /login` - Password authentication, checks 2FA requirements
- `GET /login/2fa` - Two-factor authentication page
- `POST /logout` - End user session

**Passkey/WebAuthn Endpoints:**
- `POST /webauthn/register/start` - Start passkey registration (requires session)
- `POST /webauthn/register/finish` - Complete passkey registration
- `POST /webauthn/authenticate/start` - Start passkey authentication (public)
- `POST /webauthn/authenticate/finish` - Complete passkey authentication
- `POST /webauthn/2fa/start` - Start 2FA passkey verification (requires partial session)
- `POST /webauthn/2fa/finish` - Complete 2FA passkey verification

**Passkey Management:**
- `GET /account/passkeys` - List user's registered passkeys
- `DELETE /account/passkeys/:credential_id` - Delete a passkey
- `PATCH /account/passkeys/:credential_id` - Update passkey name

**Non-Standard:**
- `GET /properties/:owner/:key` - Get property value
- `PUT /properties/:owner/:key` - Set property value
- `GET /federation/trust-anchors` - List trust anchors

### Error Handling (`src/errors.rs`)
Defines `CrabError` for internal error handling with conversions from common error types.

## Key Implementation Details

### PKCE Flow
- Only S256 code challenge method is supported (plain is rejected)
- Code challenge stored with auth code
- Code verifier validated at token endpoint by hashing and comparing

### Client Authentication
Token endpoint accepts two methods:
1. `client_secret_basic`: HTTP Basic auth (client_id:client_secret base64-encoded)
2. `client_secret_post`: Form parameters (client_id and client_secret in body)

### ID Token Claims
Generated ID tokens include:
- Standard claims: iss, sub, aud, exp, iat
- Optional: nonce (if provided in authorize request)
- at_hash: hash of access token per OIDC spec (left 128 bits of SHA-256, base64url)
- auth_time: timestamp of authentication (from session)
- amr: Authentication Method References array (e.g., ["pwd"], ["hwk"], ["pwd", "hwk"])
- acr: Authentication Context Reference ("aal1" for single-factor, "aal2" for two-factor)
- Signed with RS256, includes kid header matching JWKS

### State Management
- Authorization codes: 5 minute TTL, single-use (marked consumed)
- Access tokens: 1 hour TTL, checked for expiration and revoked flag
- Sessions: Track authentication methods (AMR), context (ACR), and MFA status
- WebAuthn challenges: 5 minute TTL, cleaned up every 5 minutes by background job
- All stored in database with timestamps

### WebAuthn/Passkey Authentication

Barycenter supports passwordless authentication using WebAuthn/FIDO2 passkeys with the following features:

**Authentication Modes:**
- **Single-factor passkey login**: Passkeys as primary authentication method
- **Two-factor authentication**: Passkeys as second factor after password login
- **Password fallback**: Traditional password authentication remains available

**Client Implementation:**
- Rust WASM module (`client-wasm/`) compiled with wasm-pack
- Browser-side WebAuthn API calls via wasm-bindgen
- Conditional UI support for autofill in Chrome 108+, Safari 16+
- Progressive enhancement: falls back to explicit button if autofill unavailable

**Passkey Storage:**
- Full `Passkey` object stored as JSON in database
- Tracks signature counter for clone detection
- Records backup state (cloud-synced vs hardware-bound)
- Supports friendly names for user management

**AMR (Authentication Method References) Values:**
- `"pwd"`: Password authentication
- `"hwk"`: Hardware-bound passkey (YubiKey, security key)
- `"swk"`: Software/cloud-synced passkey (iCloud Keychain, password manager)
- Multiple values indicate multi-factor auth (e.g., `["pwd", "hwk"]`)

**2FA Enforcement Modes:**

1. **User-Optional 2FA**: Users can enable 2FA in account settings (future UI)
2. **Admin-Enforced 2FA**: Set `users.requires_2fa = 1` via GraphQL mutation
3. **Context-Based 2FA**: Triggered by:
   - High-value scopes: "admin", "payment", "transfer", "delete"
   - Fresh authentication required: `max_age < 300` seconds
   - Can be configured per-scope or per-request

**2FA Flow:**
1. User logs in with password → creates partial session (`mfa_verified=0`)
2. If 2FA required, redirect to `/login/2fa`
3. User verifies with passkey
4. Session upgraded: `mfa_verified=1`, `acr="aal2"`, `amr=["pwd", "hwk"]`
5. Authorization proceeds, ID token includes full authentication context

## Current Implementation Status

See `docs/oidc-conformance.md` for detailed OIDC compliance requirements.

**Implemented:**
- Authorization Code flow with PKCE (S256)
- Dynamic client registration
- Token endpoint with client_secret_basic and client_secret_post
- ID Token signing (RS256) with at_hash, nonce, auth_time, AMR, and ACR claims
- UserInfo endpoint with Bearer token authentication
- Discovery and JWKS publication
- Property storage API
- User authentication with sessions
- Password authentication with argon2 hashing
- WebAuthn/passkey authentication (single-factor and two-factor)
- WASM client for browser-side WebAuthn operations
- Conditional UI/autofill for passkey login
- Three 2FA modes: user-optional, admin-enforced, context-based
- Background jobs for cleanup (sessions, tokens, challenges)
- Admin GraphQL API for user management and job triggering
- Refresh token grant with rotation
- Session-based AMR/ACR tracking

**Pending:**
- Cache-Control headers on token endpoint
- Consent flow (currently auto-consents)
- Token revocation and introspection endpoints
- OpenID Federation trust chain validation
- User account management UI

## Admin GraphQL API

The admin API is served on a separate port (default: 9091) and provides GraphQL queries and mutations for management:

**Mutations:**
```graphql
mutation {
  # Trigger background jobs manually
  triggerJob(jobName: "cleanup_expired_sessions") {
    success
    message
  }

  # Enable 2FA requirement for a user
  setUser2faRequired(username: "alice", required: true) {
    success
    message
    requires2fa
  }
}
```

**Queries:**
```graphql
query {
  # Get job execution history
  jobLogs(limit: 10, onlyFailures: false) {
    id
    jobName
    startedAt
    completedAt
    success
    recordsProcessed
  }

  # Get user 2FA status
  user2faStatus(username: "alice") {
    username
    requires2fa
    passkeyEnrolled
    passkeyCount
    passkeyEnrolledAt
  }

  # List available jobs
  availableJobs {
    name
    description
    schedule
  }
}
```

Available job names:
- `cleanup_expired_sessions` (hourly at :00)
- `cleanup_expired_refresh_tokens` (hourly at :30)
- `cleanup_expired_challenges` (every 5 minutes)

## Building the WASM Client

The passkey authentication client is written in Rust and compiled to WebAssembly:

```bash
# Install wasm-pack if not already installed
cargo install wasm-pack

# Build the WASM module
cd client-wasm
wasm-pack build --target web --out-dir ../static/wasm

# The built files will be in static/wasm/:
# - barycenter_webauthn_client_bg.wasm
# - barycenter_webauthn_client.js
# - TypeScript definitions (.d.ts files)
```

The WASM module is automatically loaded by the login page and provides:
- `supports_webauthn()`: Check if WebAuthn is available
- `supports_conditional_ui()`: Check for autofill support
- `register_passkey(options)`: Create a new passkey
- `authenticate_passkey(options, mediation)`: Authenticate with passkey

## Testing and Validation

### Manual Testing Flow

**1. Test Password Login:**
```bash
# Navigate to http://localhost:9090/login
# Enter username: admin, password: password123
# Should create session and redirect
```

**2. Test Passkey Registration:**
```bash
# After logging in with password
# Navigate to http://localhost:9090/account/passkeys
# (Future UI - currently use browser console)

# Call via JavaScript console:
fetch('/webauthn/register/start', { method: 'POST' })
  .then(r => r.json())
  .then(data => {
    // Use browser's navigator.credentials.create() with returned options
  });
```

**3. Test Passkey Authentication:**
- Navigate to `/login`
- Click on username field
- Browser should show passkey autofill (Chrome 108+, Safari 16+)
- Select a passkey to authenticate

**4. Test Admin-Enforced 2FA:**
```graphql
# Via admin API (port 9091)
mutation {
  setUser2faRequired(username: "admin", required: true) {
    success
  }
}
```

Then:
1. Log out
2. Log in with password
3. Should redirect to `/login/2fa`
4. Complete passkey verification
5. Should complete authorization with ACR="aal2"

**5. Test Context-Based 2FA:**
```bash
# Request authorization with max_age < 300
curl "http://localhost:9090/authorize?...&max_age=60"
# Should trigger 2FA even if not admin-enforced
```

### OIDC Flow Testing

```bash
# 1. Register a client
curl -X POST http://localhost:9090/connect/register \
  -H "Content-Type: application/json" \
  -d '{
    "redirect_uris": ["http://localhost:8080/callback"],
    "client_name": "Test Client"
  }'

# 2. Generate PKCE
verifier=$(openssl rand -base64 32 | tr -d '=' | tr '+/' '-_')
challenge=$(echo -n "$verifier" | openssl dgst -binary -sha256 | base64 | tr -d '=' | tr '+/' '-_')

# 3. Navigate to authorize endpoint (in browser)
http://localhost:9090/authorize?client_id=CLIENT_ID&redirect_uri=http://localhost:8080/callback&response_type=code&scope=openid&code_challenge=$challenge&code_challenge_method=S256&state=random

# 4. After redirect, exchange code for tokens
curl -X POST http://localhost:9090/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code&code=CODE&redirect_uri=http://localhost:8080/callback&client_id=CLIENT_ID&client_secret=SECRET&code_verifier=$verifier"

# 5. Decode ID token to verify AMR/ACR claims
# Use jwt.io or similar to inspect the token
```

### Expected ID Token Claims

After passkey authentication:
```json
{
  "iss": "http://localhost:9090",
  "sub": "user_subject_uuid",
  "aud": "client_id",
  "exp": 1234567890,
  "iat": 1234564290,
  "auth_time": 1234564290,
  "amr": ["hwk"],  // or ["swk"] for cloud-synced, ["pwd", "hwk"] for 2FA
  "acr": "aal1",   // or "aal2" for 2FA
  "nonce": "optional_nonce"
}
```

## Migration Guide for Existing Deployments

If you have an existing Barycenter deployment, the database will be automatically migrated when you update:

1. **Backup your database** before upgrading
2. Run the application - migrations run automatically on startup
3. New tables will be created:
   - `passkeys`: Stores registered passkeys
   - `webauthn_challenges`: Temporary challenge storage
4. Existing tables will be extended:
   - `sessions`: Added `amr`, `acr`, `mfa_verified` columns
   - `users`: Added `requires_2fa`, `passkey_enrolled_at` columns

**Post-Migration Steps:**

1. Build the WASM client:
   ```bash
   cd client-wasm
   wasm-pack build --target web --out-dir ../static/wasm
   ```

2. Restart the application to serve static files

3. Users can now register passkeys via `/account/passkeys` (future UI)

4. Enable 2FA for specific users via admin API:
   ```graphql
   mutation {
     setUser2faRequired(username: "admin", required: true) {
       success
     }
   }
   ```

**No Breaking Changes:**
- Password authentication continues to work
- Existing sessions remain valid
- ID tokens now include AMR/ACR claims (additive change)
- OIDC clients receiving new claims should handle gracefully