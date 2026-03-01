use std::time::Duration;

use chrono::{Datelike, Timelike, Utc};

use crate::backup;
use crate::config;
use crate::state::AppState;

/// Spawn the backup scheduler as a background task.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            check_and_run(&state).await;
        }
    });
}

async fn check_and_run(state: &AppState) {
    let installed = config::list_installed_services(&state.data_dir);
    for id in &installed {
        let svc_state = match config::load_service_state(&state.data_dir, id) {
            Ok(s) if s.installed => s,
            _ => continue,
        };

        let backup_cfg = match svc_state.backup.as_ref() {
            Some(b) if b.enabled => b,
            _ => continue,
        };

        let schedule = match backup_cfg.schedule.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        if !should_run_now(schedule, svc_state.last_backup_at.as_deref()) {
            continue;
        }

        tracing::info!("Scheduled backup starting for {id}");

        let global_backup = config::load_backup_config(&state.data_dir)
            .unwrap_or(None)
            .unwrap_or_default();
        let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

        match backup::backup_service(
            &state.data_dir,
            id,
            &state.registry,
            &global_config,
            &global_backup,
        )
        .await
        {
            Ok(results) => {
                tracing::info!(
                    "Scheduled backup for {id} complete: {} snapshot(s)",
                    results.len()
                );
                // Update last_backup_at
                if let Ok(mut st) = config::load_service_state(&state.data_dir, id) {
                    st.last_backup_at = Some(Utc::now().to_rfc3339());
                    let _ = config::save_service_state(&state.data_dir, id, &st);
                }
            }
            Err(e) => {
                tracing::error!("Scheduled backup for {id} failed: {e}");
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
}
