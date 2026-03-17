use crate::types::{
    AccountAction, AccountActionKind, AccountActionSummary, Provider, ProviderCapabilities,
    StoredAccount, UsageInfo,
};

pub fn summarize_account_history(
    account: &StoredAccount,
    history: &[AccountAction],
) -> (Option<AccountActionSummary>, Option<AccountActionSummary>) {
    let last_action = history
        .iter()
        .rev()
        .find(|action| action.account_id.as_deref() == Some(account.id.as_str()))
        .map(AccountActionSummary::from_action);

    let last_refresh_error = history
        .iter()
        .rev()
        .find(|action| {
            action.account_id.as_deref() == Some(account.id.as_str())
                && matches!(
                    action.kind,
                    AccountActionKind::RefreshError | AccountActionKind::RefreshRecovered
                )
        })
        .filter(|action| action.kind == AccountActionKind::RefreshError)
        .map(AccountActionSummary::from_action);

    (last_action, last_refresh_error)
}

pub fn recommend_best_account(
    accounts: &[StoredAccount],
    usage_by_account: &[UsageInfo],
    provider: Provider,
) -> Option<String> {
    accounts
        .iter()
        .filter(|account| {
            account.provider == provider
                && ProviderCapabilities::from_provider(account.provider).supports_switch
        })
        .filter_map(|account| {
            let usage = usage_by_account
                .iter()
                .find(|usage| usage.account_id == account.id && usage.error.is_none())?;
            let remaining = (100.0 - usage.primary_used_percent.unwrap_or(100.0)).max(0.0);
            let reset = usage.primary_resets_at.unwrap_or(i64::MAX);
            Some((account.id.clone(), remaining, reset, account.name.clone()))
        })
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| right.3.cmp(&left.3))
        })
        .map(|candidate| candidate.0)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{recommend_best_account, summarize_account_history};
    use crate::types::{
        AccountAction, AccountActionKind, Provider, ProviderCapabilities, StoredAccount, UsageInfo,
    };

    fn usage(account_id: &str, used: f64, resets_at: i64) -> UsageInfo {
        UsageInfo {
            account_id: account_id.to_string(),
            plan_type: Some("pro".to_string()),
            primary_used_percent: Some(used),
            primary_window_minutes: Some(300),
            primary_resets_at: Some(resets_at),
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: Some("healthy".to_string()),
            daily_stats: None,
            skipped: false,
            error: None,
        }
    }

    #[test]
    fn recommendation_prefers_more_remaining_quota_then_earlier_reset() {
        let first = StoredAccount::new_api_key("A".to_string(), "sk-a".to_string());
        let second = StoredAccount::new_api_key("B".to_string(), "sk-b".to_string());

        let recommended = recommend_best_account(
            &[first.clone(), second.clone()],
            &[usage(&first.id, 65.0, 10_000), usage(&second.id, 20.0, 20_000)],
            Provider::Codex,
        );

        assert_eq!(recommended.as_deref(), Some(second.id.as_str()));
    }

    #[test]
    fn recommendation_skips_non_switchable_provider_accounts() {
        let session = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc".to_string(),
        );

        let recommended =
            recommend_best_account(&[session.clone()], &[usage(&session.id, 10.0, 10_000)], Provider::Gemini);

        assert!(recommended.is_none());
    }

    #[test]
    fn summarize_history_returns_last_action_and_last_refresh_error() {
        let account = StoredAccount::new_api_key("Codex".to_string(), "sk".to_string());
        let switched_at = Utc.with_ymd_and_hms(2026, 3, 16, 10, 0, 0).unwrap();
        let errored_at = Utc.with_ymd_and_hms(2026, 3, 16, 11, 0, 0).unwrap();

        let history = vec![
            AccountAction {
                id: "switch".to_string(),
                account_id: Some(account.id.clone()),
                provider: Some(Provider::Codex),
                kind: AccountActionKind::Switch,
                created_at: switched_at,
                summary: "Switched to Codex".to_string(),
                detail: None,
                is_error: false,
            },
            AccountAction {
                id: "error".to_string(),
                account_id: Some(account.id.clone()),
                provider: Some(Provider::Codex),
                kind: AccountActionKind::RefreshError,
                created_at: errored_at,
                summary: "Usage refresh failed".to_string(),
                detail: Some("401".to_string()),
                is_error: true,
            },
        ];

        let (last_action, last_error) = summarize_account_history(&account, &history);

        assert_eq!(last_action.as_ref().map(|action| action.kind), Some(AccountActionKind::RefreshError));
        assert_eq!(last_error.as_ref().map(|action| action.kind), Some(AccountActionKind::RefreshError));
        assert_eq!(last_error.as_ref().and_then(|action| action.detail.as_deref()), Some("401"));
    }

    #[test]
    fn summarize_history_clears_last_refresh_error_after_recovery() {
        let account = StoredAccount::new_api_key("Codex".to_string(), "sk".to_string());
        let errored_at = Utc.with_ymd_and_hms(2026, 3, 16, 11, 0, 0).unwrap();
        let recovered_at = Utc.with_ymd_and_hms(2026, 3, 16, 12, 0, 0).unwrap();

        let history = vec![
            AccountAction {
                id: "error".to_string(),
                account_id: Some(account.id.clone()),
                provider: Some(Provider::Codex),
                kind: AccountActionKind::RefreshError,
                created_at: errored_at,
                summary: "Usage refresh failed".to_string(),
                detail: Some("403".to_string()),
                is_error: true,
            },
            AccountAction {
                id: "recovered".to_string(),
                account_id: Some(account.id.clone()),
                provider: Some(Provider::Codex),
                kind: AccountActionKind::RefreshRecovered,
                created_at: recovered_at,
                summary: "Usage refresh recovered".to_string(),
                detail: None,
                is_error: false,
            },
        ];

        let (last_action, last_error) = summarize_account_history(&account, &history);

        assert_eq!(
            last_action.as_ref().map(|action| action.kind),
            Some(AccountActionKind::RefreshRecovered)
        );
        assert!(last_error.is_none());
    }

    #[test]
    fn provider_capabilities_expose_switch_support() {
        let codex = ProviderCapabilities::from_provider(Provider::Codex);
        let gemini = ProviderCapabilities::from_provider(Provider::Gemini);

        assert!(codex.supports_switch);
        assert!(!gemini.supports_switch);
        assert!(gemini.supports_usage);
    }
}
