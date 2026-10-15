//! Background task: every [`REFRESH_PERIOD`], refresh the OAuth tokens that expire within
//! [`REFRESH_MARGIN_SECS`](super::REFRESH_MARGIN_SECS) — GitLab access tokens live ~2 h, GitHub's
//! expiring user tokens 8 h — so the credential helper always hands git a valid token.

use std::time::Duration;

use nanomgg_providers::now_unix;
use tauri::{AppHandle, Manager};

use super::{AuthRuntime, REFRESH_PERIOD, service};
use crate::state::AppState;

/// Delay before the first pass, so startup is not slowed down by network calls.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(15);

pub async fn run(app: AppHandle) {
    tokio::time::sleep(FIRST_PASS_DELAY).await;
    let mut ticker = tokio::time::interval(REFRESH_PERIOD);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let (Some(st), Some(rt)) = (app.try_state::<AppState>(), app.try_state::<AuthRuntime>())
        else {
            continue;
        };
        let refreshed = service::refresh_due(&st, &rt, now_unix()).await;
        if refreshed > 0 {
            tracing::info!(refreshed, "background token refresh");
        }
    }
}
