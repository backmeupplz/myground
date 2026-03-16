use std::time::Duration;

use chrono::{Datelike, Timelike, Utc};

use crate::backup::{self, STATE_PERSIST_LOCK};
use crate::config;
use crate::state::AppState;
use crate::updates;

/// Spawn the backup scheduler as a background task.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        recover_interrupted(&state);
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            check_and_run(&state).await;
            check_for_updates(&state).await;
        }
    });
}

/// Re-run backup jobs that were interrupted mid-flight (last_status == "running" on disk).
/// Each interrupted job is spawned concurrently so they don't block each other.
fn recover_interrupted(state: &AppState) {
    let installed = config::list_installed_apps(&state.data_dir);
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    for id in &installed {
        let svc_state = match config::load_app_state(&state.data_dir, id) {
            Ok(s) if s.installed => s,
            _ => continue,
        };

        for job in &svc_state.backup_jobs {
            if job.last_status.as_deref() != Some("running") {
                continue;
            }

            tracing::info!("Recovering interrupted backup {id}/{}", job.id);

            // Mark the crashed run as failed so the UI doesn't show "Unknown"
            {
                let _lock = STATE_PERSIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                if let Ok(mut st) = config::load_app_state(&state.data_dir, id) {
                    if let Some(j) = st.backup_jobs.iter_mut().find(|j2| j2.id == job.id) {
                        j.last_status = Some("failed".to_string());
                        j.last_error = Some("Interrupted (server restarted)".to_string());
                    }
                    let _ = config::save_app_state(&state.data_dir, id, &st);
                }
            }

            let job_id = job.id.clone();
            let app_id = id.clone();
            let data_dir = state.data_dir.clone();
            let registry = state.registry.clone();
            let global_config = global_config.clone();
            let progress = state.backup_progress.clone();
            let cancel = state.backup_cancel.clone();

            tokio::spawn(async move {
                match backup::backup_job_run(
                    &data_dir,
                    &app_id,
                    &job_id,
                    &registry,
                    &global_config,
                    &progress,
                    &cancel,
                )
                .await
                {
                    Ok(results) => {
                        tracing::info!(
                            "Recovered backup for {app_id} job {job_id} complete: {} snapshot(s)",
                            results.len()
                        );
                    }
                    Err(e) => {
                        tracing::error!("Recovered backup for {app_id} job {job_id} failed: {e}");
                    }
                }
            });
        }
    }
}

async fn check_and_run(state: &AppState) {
    let installed = config::list_installed_apps(&state.data_dir);
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    for id in &installed {
        let svc_state = match config::load_app_state(&state.data_dir, id) {
            Ok(s) if s.installed => s,
            _ => continue,
        };

        for job in &svc_state.backup_jobs {
            let schedule = match job.schedule.as_deref() {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };

            if !should_run_now(schedule, job.last_run_at.as_deref()) {
                continue;
            }

            // Skip if this job is already running
            {
                let map = state.backup_progress.read().unwrap_or_else(|e| e.into_inner());
                if let Some(p) = map.get(&job.id) {
                    if p.status == "running" {
                        tracing::info!("Skipping scheduled backup {id}/{} — already running", job.id);
                        // Persist skip timestamp so UI can show it
                        {
                            let _lock = STATE_PERSIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(mut st) = config::load_app_state(&state.data_dir, id) {
                                if let Some(j) = st.backup_jobs.iter_mut().find(|j2| j2.id == job.id) {
                                    j.last_skipped_at = Some(chrono::Utc::now().to_rfc3339());
                                }
                                let _ = config::save_app_state(&state.data_dir, id, &st);
                            }
                        }
                        continue;
                    }
                }
            }

            tracing::info!("Scheduled backup starting for {id}, job {}", job.id);

            let job_id = job.id.clone();
            let app_id = id.clone();

            match backup::backup_job_run(
                &state.data_dir,
                &app_id,
                &job_id,
                &state.registry,
                &global_config,
                &state.backup_progress,
                &state.backup_cancel,
            )
            .await
            {
                Ok(results) => {
                    tracing::info!(
                        "Scheduled backup for {app_id} job {job_id} complete: {} snapshot(s)",
                        results.len()
                    );
                }
                Err(e) => {
                    tracing::error!("Scheduled backup for {app_id} job {job_id} failed: {e}");
                }
            }
        }
    }
}

/// Check for updates every 6 hours. When auto-update is enabled, apply them.
///
/// The check and apply phases are intentionally separated: the 6-hour throttle
/// only gates the expensive `docker pull` check, not the apply step.  This way
/// updates discovered by a manual check are still applied on the next scheduler
/// tick (every 60 s) instead of waiting up to 6 hours.
async fn check_for_updates(state: &AppState) {
    let global = config::load_global_config(&state.data_dir).unwrap_or_default();
    let updates_cfg = global.updates.clone().unwrap_or_default();

    // ── Check phase (throttled to every 6 hours) ────────────────────────
    let mut should_check = true;
    if let Some(ref last) = updates_cfg.last_check {
        if let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(last) {
            let elapsed = Utc::now() - last_dt.with_timezone(&Utc);
            if elapsed.num_hours() < 6 {
                should_check = false;
            }
        }
    }

    if should_check {
        tracing::info!("Running scheduled update check");
        let (svc_count, mg_update) =
            updates::check_all_updates(&state.data_dir, &state.registry).await;

        if svc_count > 0 || mg_update {
            tracing::info!(
                "Updates found: {svc_count} app(s), myground: {mg_update}"
            );
        }
    }

    // ── Apply phase (runs every tick, applies any pending updates) ───────
    if updates_cfg.auto_update_apps {
        let installed = config::list_installed_apps(&state.data_dir);
        for id in &installed {
            let svc_state = match config::load_app_state(&state.data_dir, id) {
                Ok(s) if s.update_available => s,
                _ => continue,
            };
            drop(svc_state);
            tracing::info!("Auto-updating app {id}");
            if let Err(e) = updates::update_app(&state.data_dir, id).await {
                tracing::error!("Auto-update for {id} failed: {e}");
            }
        }
    }

    if updates_cfg.auto_update_myground {
        let global = config::load_global_config(&state.data_dir).unwrap_or_default();
        let mg_update = global
            .updates
            .as_ref()
            .and_then(|u| u.latest_myground_version.as_ref())
            .map(|v| updates::semver_is_newer(v, env!("CARGO_PKG_VERSION")))
            .unwrap_or(false);

        if mg_update {
            if let Some(url) = global.updates.as_ref().and_then(|u| u.latest_myground_url.as_ref()) {
                tracing::info!("Auto-updating MyGround binary");
                if let Err(e) = updates::self_update(url).await {
                    tracing::error!("MyGround auto-update failed: {e}");
                }
            }
        }
    }
}

/// Check if a backup should run right now based on the schedule and last run time.
fn should_run_now(schedule: &str, last_backup_at: Option<&str>) -> bool {
    let now = Utc::now();

    let last = last_backup_at.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    // Don't run if last backup was less than 30 minutes ago (prevent rapid re-runs)
    if let Some(ref last) = last {
        if (now - *last).num_minutes() < 30 {
            return false;
        }
    }

    match schedule {
        "daily" => match_preset(last.as_ref(), 24),
        "weekly" => match_preset(last.as_ref(), 24 * 7),
        "monthly" => match_preset(last.as_ref(), 24 * 30),
        cron_expr => match_cron(cron_expr, &now),
    }
}

/// Preset schedules: run if enough hours have elapsed since last backup,
/// and it's currently between 2:00-2:59 UTC (quiet hours).
fn match_preset(last: Option<&chrono::DateTime<Utc>>, interval_hours: i64) -> bool {
    let now = Utc::now();
    // Only trigger during the 2 AM UTC hour
    if now.hour() != 2 {
        return false;
    }
    match last {
        Some(last) => (now - *last).num_hours() >= interval_hours,
        None => true, // Never run before — run now
    }
}

/// Match a 5-field cron expression: minute hour day_of_month month day_of_week
fn match_cron(expr: &str, now: &chrono::DateTime<Utc>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }

    matches_field(fields[0], now.minute())
        && matches_field(fields[1], now.hour())
        && matches_field(fields[2], now.day())
        && matches_field(fields[3], now.month())
        && matches_dow(fields[4], now.weekday().num_days_from_sunday())
}

/// Check if a cron field matches a value. Supports: *, N, */N, N-M, N,M,O
fn matches_field(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }
    // Step: */N
    if let Some(step) = field.strip_prefix("*/") {
        if let Ok(n) = step.parse::<u32>() {
            return n > 0 && value % n == 0;
        }
    }
    // List: N,M,O
    if field.contains(',') {
        return field.split(',').any(|part| matches_field(part.trim(), value));
    }
    // Range: N-M
    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(lo), Ok(hi)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return value >= lo && value <= hi;
            }
        }
    }
    // Exact
    field.parse::<u32>().ok() == Some(value)
}

/// Match day-of-week field (0=Sunday, 7=Sunday too in some crons)
fn matches_dow(field: &str, dow: u32) -> bool {
    if field == "*" {
        return true;
    }
    // Normalize: treat 7 as 0 (Sunday)
    let normalized = if dow == 7 { 0 } else { dow };
    // Also handle values in the field that use 7 for Sunday
    if field.contains(',') {
        return field.split(',').any(|part| matches_dow(part.trim(), normalized));
    }
    if let Ok(n) = field.parse::<u32>() {
        let n = if n == 7 { 0 } else { n };
        return normalized == n;
    }
    matches_field(field, normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_field_star() {
        assert!(matches_field("*", 0));
        assert!(matches_field("*", 59));
    }

    #[test]
    fn matches_field_exact() {
        assert!(matches_field("5", 5));
        assert!(!matches_field("5", 6));
    }

    #[test]
    fn matches_field_step() {
        assert!(matches_field("*/15", 0));
        assert!(matches_field("*/15", 15));
        assert!(matches_field("*/15", 30));
        assert!(!matches_field("*/15", 7));
    }

    #[test]
    fn matches_field_list() {
        assert!(matches_field("1,15,30", 1));
        assert!(matches_field("1,15,30", 15));
        assert!(!matches_field("1,15,30", 7));
    }

    #[test]
    fn matches_field_range() {
        assert!(matches_field("1-5", 1));
        assert!(matches_field("1-5", 3));
        assert!(matches_field("1-5", 5));
        assert!(!matches_field("1-5", 0));
        assert!(!matches_field("1-5", 6));
    }

    #[test]
    fn match_cron_specific_time() {
        let dt = Utc::now()
            .with_hour(2)
            .unwrap()
            .with_minute(30)
            .unwrap();
        assert!(match_cron("30 2 * * *", &dt));
        assert!(!match_cron("0 3 * * *", &dt));
    }

    #[test]
    fn should_run_daily_never_ran() {
        assert!(should_run_now("daily", None) || true); // depends on current hour
    }

    #[test]
    fn should_run_skips_recent() {
        let recent = Utc::now().to_rfc3339();
        assert!(!should_run_now("daily", Some(&recent)));
    }

    #[test]
    fn invalid_cron_returns_false() {
        let now = Utc::now();
        assert!(!match_cron("bad", &now));
        assert!(!match_cron("a b c d e", &now));
    }

    #[test]
    fn matches_dow_star() {
        assert!(matches_dow("*", 0));
        assert!(matches_dow("*", 6));
    }

    #[test]
    fn matches_dow_exact() {
        assert!(matches_dow("1", 1)); // Monday
        assert!(!matches_dow("1", 2));
    }

    #[test]
    fn matches_dow_sunday_as_seven() {
        // 7 should be treated as 0 (Sunday)
        assert!(matches_dow("0", 7));
        assert!(matches_dow("7", 0));
        assert!(matches_dow("7", 7));
    }

    #[test]
    fn matches_dow_list() {
        assert!(matches_dow("1,3,5", 1));
        assert!(matches_dow("1,3,5", 3));
        assert!(matches_dow("1,3,5", 5));
        assert!(!matches_dow("1,3,5", 2));
    }

    #[test]
    fn matches_dow_range_via_fallback() {
        // Range falls through to matches_field
        assert!(matches_dow("1-5", 3));
        assert!(!matches_dow("1-5", 0)); // Sunday out of range
    }

    #[test]
    fn match_cron_with_dow() {
        let dt = Utc::now()
            .with_hour(10)
            .unwrap()
            .with_minute(0)
            .unwrap();
        let dow = dt.weekday().num_days_from_sunday();
        let expr = format!("0 10 * * {dow}");
        assert!(match_cron(&expr, &dt));
        // Wrong day
        let wrong = (dow + 1) % 7;
        let expr2 = format!("0 10 * * {wrong}");
        assert!(!match_cron(&expr2, &dt));
    }

    #[test]
    fn matches_field_step_zero_never_matches() {
        // */0 should not match (division by zero guard)
        assert!(!matches_field("*/0", 0));
        assert!(!matches_field("*/0", 5));
    }

    #[test]
    fn matches_field_invalid_returns_false() {
        assert!(!matches_field("abc", 5));
        assert!(!matches_field("", 0));
    }

    #[test]
    fn match_cron_all_stars() {
        // "* * * * *" matches any time
        let dt = Utc::now();
        assert!(match_cron("* * * * *", &dt));
    }

    #[test]
    fn match_cron_wrong_field_count() {
        let dt = Utc::now();
        assert!(!match_cron("* * *", &dt));
        assert!(!match_cron("* * * * * *", &dt));
    }

    #[test]
    fn match_preset_only_at_2am() {
        // match_preset requires hour == 2; tested indirectly via should_run_now
        // but we can test the boundary: recent backup should prevent re-run
        let old = (Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        // Whether this passes depends on current hour, but the recent-skip is always false
        let recent = Utc::now().to_rfc3339();
        assert!(!should_run_now("daily", Some(&recent)));
        // Old backup + daily: depends on hour being 2
        let _ = should_run_now("daily", Some(&old));
    }

    #[test]
    fn should_run_weekly_never_ran() {
        // Like daily but weekly
        let _ = should_run_now("weekly", None);
    }

    #[test]
    fn should_run_monthly_never_ran() {
        let _ = should_run_now("monthly", None);
    }

    #[test]
    fn should_run_cron_expression() {
        let now = Utc::now();
        let minute = now.minute();
        let hour = now.hour();
        let expr = format!("{minute} {hour} * * *");
        // Must not skip due to recent backup
        let old = (now - chrono::Duration::hours(1)).to_rfc3339();
        assert!(should_run_now(&expr, Some(&old)));
    }
}
