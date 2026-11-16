---
title: Authentication
parent: Guides
nav_order: 3
---

The service signs people in through gatehouse with `quench-auth`; it never sees a password. Without gatehouse the service runs for one local person.

## With gatehouse (`SERVICE_AUTH_ENABLED=true`)

The service builds `JwtConfig::init()` and `SsoConfig::init()` from its environment (`SERVICE_NAME`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`) and registers these routes under the base path:

| Route | What it does |
|---|---|
| `GET /ui/login?redirect=<path>` | starts the sign-in: sends the browser to gatehouse's `/api/v1/authorize` with the client `lore-office` and a PKCE challenge. With a still valid refresh cookie it skips the login page |
| `GET /ui/auth/callback` | completes the exchange, sets the session and refresh cookies |
| `GET /ui/logout` | clears the cookies (`quench-auth`'s `logout_delegation`) |
| `GET /ui/home` | where `quench-auth` always ends; sends the person back to the page they came from, else to the launcher |

`quench-auth` ignores `?redirect=` and always ends at `/ui/home`. So `/ui/login` stores the wanted page in a cookie `lore_return` (percent-encoded, `Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=600`) and `/ui/home` reads it, clears it and redirects there. The target must start with the base path followed by `/`, be at most 300 characters and contain no `//`, no backslash and no control character; anything else goes to the launcher.

The session is the cookie `forge_session` (`AUTH_COOKIE_NAME` to change it) holding gatehouse's access token; `forge_refresh` (`AUTH_REFRESH_COOKIE_NAME`) holds the refresh token for `REFRESH_TOKEN_TTL_SECS` (604800 by default).

## Who is asking

A request is accepted as a person when the session token verifies against gatehouse's keys and its audience includes `SERVICE_NAME` (`lore-office`). The audience comes from the person's permissions in gatehouse: the service `lore-office` with the actions `read` and `write`, granted by the viewer and editor templates of the forge overlay.

A person signed in at gatehouse whose token lacks that audience is not accepted. The page then says so in the Cloud panel: "Signed in as <name>: this account has no access to the cloud; ask an administrator", with Sign out. A person who is not signed in sees Sign in. API calls without an accepted person get `401 sign in first`.

The service does not check the actions `read` and `write` themselves: every accepted person may use their own storage.

## Without gatehouse

`user_of` gives every request the name `local`, unless the header `X-Lore-User` (not empty, shorter than 80 characters) names someone, so several people can be tried. No sign-in routes exist (`/ui/login` answers 404 `sign-in is not set up`) and the Cloud panel says "Local use: no sign-in on this server".

## The warehouse identity

Storing documents in warehouse uses a second client, `lore-office-warehouse` (client credentials, scope `warehouse`), configured by `WAREHOUSE_CLIENT_ID` and `WAREHOUSE_CLIENT_SECRET`. It makes and fills each person's storage; the person's own token is not used for warehouse. See [Data](../services/lore-office/data.md).
