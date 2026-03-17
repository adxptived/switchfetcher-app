use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;

use crate::types::DailyStats;

const CLAUDE_PROJECTS_PATH: &str = ".claude/projects";
const SESSION_GAP_MINUTES: i64 = 30;

#[derive(Clone, Copy)]
struct Pricing {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

#[derive(Default)]
struct ParsedEntry {
    timestamp: Option<DateTime<Utc>>,
    model: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
}

pub fn parse_daily_stats() -> Result<DailyStats> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let projects_root = home.join(CLAUDE_PROJECTS_PATH);
    parse_daily_stats_from_path_and_now(&projects_root, Utc::now())
}

fn parse_daily_stats_from_path_and_now(root: &Path, now: DateTime<Utc>) -> Result<DailyStats> {
    if !root.exists() {
        anyhow::bail!("Claude projects directory not found: {}", root.display());
    }

    let mut stats = DailyStats {
        today_input_tokens: 0,
        today_output_tokens: 0,
        today_cache_creation_tokens: 0,
        today_cache_read_tokens: 0,
        today_cost_usd: 0.0,
        today_session_count: 0,
        yesterday_input_tokens: 0,
        yesterday_output_tokens: 0,
        yesterday_cost_usd: 0.0,
    };

    let mut today_timestamps = Vec::new();
    let mut files = Vec::new();
    collect_recent_jsonl_files(root, now, &mut files)?;

    let today_start = start_of_day_utc(now);
    let yesterday_start = today_start - Duration::days(1);

    for path in files {
        let file = File::open(&path)
            .with_context(|| format!("Failed to open Claude history file: {}", path.display()))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let entry = parse_entry(&value);
            let timestamp = match entry.timestamp {
                Some(value) => value,
                None => continue,
            };

            if timestamp >= today_start {
                apply_to_today(&mut stats, &entry);
                today_timestamps.push(timestamp);
            } else if timestamp >= yesterday_start {
                apply_to_yesterday(&mut stats, &entry);
            }
        }
    }

    stats.today_session_count = count_sessions(&mut today_timestamps);
    Ok(stats)
}

fn collect_recent_jsonl_files(
    root: &Path,
    now: DateTime<Utc>,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let cutoff = now - Duration::days(2);

    for entry in fs::read_dir(root)
        .with_context(|| format!("Failed to read Claude projects directory: {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            collect_recent_jsonl_files(&path, now, files)?;
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }

        let modified = metadata
            .modified()
            .ok()
            .and_then(system_time_to_utc)
            .unwrap_or(now);
        if modified >= cutoff {
            files.push(path);
        }
    }

    Ok(())
}

fn system_time_to_utc(value: SystemTime) -> Option<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp(value.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64, 0)
}

fn parse_entry(value: &Value) -> ParsedEntry {
    ParsedEntry {
        timestamp: value.get("timestamp").and_then(parse_timestamp_value),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        input_tokens: numeric_field(value, &["input_tokens"]),
        output_tokens: numeric_field(value, &["output_tokens"]),
        cache_creation_input_tokens: numeric_field(
            value,
            &["cache_creation_input_tokens", "cache_creation_tokens"],
        ),
        cache_read_input_tokens: numeric_field(value, &["cache_read_input_tokens", "cache_read_tokens"]),
    }
}

fn parse_timestamp_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|value| value.with_timezone(&Utc)),
        Value::Number(number) => {
            if let Some(seconds) = number.as_i64() {
                Utc.timestamp_opt(seconds, 0).single()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn numeric_field(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

fn apply_to_today(stats: &mut DailyStats, entry: &ParsedEntry) {
    stats.today_input_tokens += entry.input_tokens;
    stats.today_output_tokens += entry.output_tokens;
    stats.today_cache_creation_tokens += entry.cache_creation_input_tokens;
    stats.today_cache_read_tokens += entry.cache_read_input_tokens;
    stats.today_cost_usd += entry_cost_usd(entry);
}

fn apply_to_yesterday(stats: &mut DailyStats, entry: &ParsedEntry) {
    stats.yesterday_input_tokens += entry.input_tokens;
    stats.yesterday_output_tokens += entry.output_tokens;
    stats.yesterday_cost_usd += entry_cost_usd(entry);
}

fn entry_cost_usd(entry: &ParsedEntry) -> f64 {
    let pricing = pricing_for_model(entry.model.as_deref());
    let tokens_to_million = 1_000_000.0;

    (entry.input_tokens as f64 / tokens_to_million) * pricing.input
        + (entry.output_tokens as f64 / tokens_to_million) * pricing.output
        + (entry.cache_creation_input_tokens as f64 / tokens_to_million) * pricing.cache_write
        + (entry.cache_read_input_tokens as f64 / tokens_to_million) * pricing.cache_read
}

fn pricing_for_model(model: Option<&str>) -> Pricing {
    let model = model.unwrap_or_default();
    if model.starts_with("claude-3-5-sonnet") {
        Pricing {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.30,
        }
    } else if model.starts_with("claude-3-7-sonnet") {
        Pricing {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.30,
        }
    } else if model.starts_with("claude-3-opus") {
        Pricing {
            input: 15.0,
            output: 75.0,
            cache_write: 18.75,
            cache_read: 1.50,
        }
    } else if model.starts_with("claude-3-5-haiku") {
        Pricing {
            input: 0.80,
            output: 4.0,
            cache_write: 1.00,
            cache_read: 0.08,
        }
    } else if model.starts_with("claude-3-haiku") {
        Pricing {
            input: 0.25,
            output: 1.25,
            cache_write: 0.30,
            cache_read: 0.03,
        }
    } else {
        Pricing {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.30,
        }
    }
}

fn count_sessions(timestamps: &mut [DateTime<Utc>]) -> u32 {
    if timestamps.is_empty() {
        return 0;
    }

    timestamps.sort_unstable();

    let mut sessions = 1;
    for pair in timestamps.windows(2) {
        if pair[1] - pair[0] > Duration::minutes(SESSION_GAP_MINUTES) {
            sessions += 1;
        }
    }

    sessions
}

fn start_of_day_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let date = now.date_naive();
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight should be valid"))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use chrono::{Duration, TimeZone, Utc};

    use super::parse_daily_stats_from_path_and_now;

    fn make_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "switchfetcher-claude-daily-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn aggregates_today_and_yesterday_costs_and_sessions() {
        let root = make_temp_dir();
        let project_dir = root.join("project-a");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        let now = Utc
            .with_ymd_and_hms(2026, 3, 16, 12, 0, 0)
            .single()
            .expect("fixed timestamp should be valid");
        let today_1 = now - Duration::hours(1);
        let today_2 = today_1 + Duration::minutes(10);
        let today_3 = today_1 + Duration::minutes(45);
        let yesterday = now - Duration::days(1);

        let content = format!(
            concat!(
                "{{\"timestamp\":\"{}\",\"model\":\"claude-3-5-sonnet\",\"input_tokens\":1000,\"output_tokens\":2000,\"cache_creation_input_tokens\":500,\"cache_read_input_tokens\":250}}\n",
                "{{\"timestamp\":\"{}\",\"model\":\"claude-3-5-sonnet\",\"input_tokens\":2000,\"output_tokens\":1000}}\n",
                "{{\"timestamp\":\"{}\",\"model\":\"claude-3-5-sonnet\",\"input_tokens\":500,\"output_tokens\":500}}\n",
                "{{\"timestamp\":\"{}\",\"model\":\"claude-3-haiku\",\"input_tokens\":4000,\"output_tokens\":2000}}\n"
            ),
            today_1.to_rfc3339(),
            today_2.to_rfc3339(),
            today_3.to_rfc3339(),
            yesterday.to_rfc3339(),
        );

        fs::write(project_dir.join("session.jsonl"), content).expect("jsonl should be written");

        let stats = parse_daily_stats_from_path_and_now(&root, now).expect("stats should parse");

        assert_eq!(stats.today_input_tokens, 3500);
        assert_eq!(stats.today_output_tokens, 3500);
        assert_eq!(stats.today_cache_creation_tokens, 500);
        assert_eq!(stats.today_cache_read_tokens, 250);
        assert_eq!(stats.today_session_count, 2);
        assert!((stats.today_cost_usd - 0.06495).abs() < 1e-9);
        assert_eq!(stats.yesterday_input_tokens, 4000);
        assert_eq!(stats.yesterday_output_tokens, 2000);
        assert!((stats.yesterday_cost_usd - 0.0035).abs() < 1e-9);

        fs::remove_dir_all(root).ok();
    }
}
