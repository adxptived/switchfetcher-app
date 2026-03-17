import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings,
  NotificationPermissionState,
  Provider,
  UsageInfo,
  AccountWithUsage,
} from "../types";
import {
  addAccountFromFile,
  addSessionCookieAccount,
  cancelLogin,
  completeLogin,
  deleteAccount as deleteAccountIpc,
  deleteAccountsBulk,
  exportAccountsFullEncryptedFile as exportAccountsFullEncryptedFileIpc,
  exportAccountsSlimText as exportAccountsSlimTextIpc,
  exportSelectedAccountsFullEncryptedFile as exportSelectedAccountsFullEncryptedFileIpc,
  exportSelectedAccountsSlimText as exportSelectedAccountsSlimTextIpc,
  getAppSettings,
  getBestAccountRecommendation as getBestAccountRecommendationIpc,
  getDiagnostics as getDiagnosticsIpc,
  getNotificationPermissionState as getNotificationPermissionStateIpc,
  getProviderCapabilities as getProviderCapabilitiesIpc,
  getUsage,
  importAccountsFullEncryptedFile as importAccountsFullEncryptedFileIpc,
  importAccountsSlimText as importAccountsSlimTextIpc,
  importClaudeCredentials as importClaudeCredentialsIpc,
  importClaudeCredentialsFromPath as importClaudeCredentialsFromPathIpc,
  importGeminiCredentials as importGeminiCredentialsIpc,
  importGeminiCredentialsFromPath as importGeminiCredentialsFromPathIpc,
  listAccountHistory as listAccountHistoryIpc,
  listAccounts,
  refreshAllAccountsUsage,
  refreshSelectedAccountsUsage,
  renameAccount as renameAccountIpc,
  repairAccountSecret as repairAccountSecretIpc,
  requestNotificationPermission as requestNotificationPermissionIpc,
  sendTestNotification as sendTestNotificationIpc,
  setAccountTags as setAccountTagsIpc,
  setProviderHidden as setProviderHiddenIpc,
  startLogin,
  switchAccount as switchAccountIpc,
  updateAppSettings as updateAppSettingsIpc,
  warmupAccount as warmupAccountIpc,
  warmupAllAccounts as warmupAllAccountsIpc,
} from "../ipc";

export function useAccounts() {
  const [accounts, setAccounts] = useState<AccountWithUsage[]>([]);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [notificationPermission, setNotificationPermission] =
    useState<NotificationPermissionState>("default");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadAccounts = useCallback(async (preserveUsage = false, markUsageLoading = false) => {
    try {
      setLoading(true);
      setError(null);
      const accountList = await listAccounts();
      
      if (preserveUsage) {
        // Preserve existing usage data when just updating account info
        setAccounts((prev) => {
          const usageMap = new Map(prev.map((a) => [a.id, a.usage]));
          const usageLoadingMap = new Map(prev.map((a) => [a.id, a.usageLoading]));
          return accountList.map((a) => ({
            ...a,
            usage: usageMap.get(a.id),
            usageLoading: markUsageLoading || usageLoadingMap.get(a.id) || false,
          }));
        });
      } else {
        setAccounts(
          accountList.map((a) => ({
            ...a,
            usageLoading: markUsageLoading,
          }))
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const refreshUsage = useCallback(async (markLoading = true) => {
    try {
      if (markLoading) {
        setAccounts((prev) =>
          prev.map((account) => ({ ...account, usageLoading: true }))
        );
      }
      const usageList = await refreshAllAccountsUsage();
      setAccounts((prev) =>
        prev.map((account) => {
          const usage = usageList.find((u) => u.account_id === account.id);
          return usage
            ? { ...account, usage, usageLoading: false }
            : { ...account, usageLoading: false };
        })
      );
    } catch (err) {
      setAccounts((prev) =>
        prev.map((account) => ({ ...account, usageLoading: false }))
      );
      console.error("Failed to refresh usage:", err);
      throw err;
    }
  }, []);

  const loadAppSettings = useCallback(async () => {
    const settings = await getAppSettings();
    setAppSettings(settings);
    return settings;
  }, []);

  const updateAppSettings = useCallback(async (settings: AppSettings) => {
    const saved = await updateAppSettingsIpc(settings);
    setAppSettings(saved);
    return saved;
  }, []);

  const getNotificationPermissionState = useCallback(async () => {
    const state = await getNotificationPermissionStateIpc();
    setNotificationPermission(state);
    return state;
  }, []);

  const requestNotificationPermission = useCallback(async () => {
    const state = await requestNotificationPermissionIpc();
    setNotificationPermission(state);
    return state;
  }, []);

  const sendTestNotification = useCallback(async () => {
    await sendTestNotificationIpc();
  }, []);

  const refreshSingleUsage = useCallback(async (accountId: string) => {
    try {
      setAccounts((prev) =>
        prev.map((a) =>
          a.id === accountId ? { ...a, usageLoading: true } : a
        )
      );
      const usage = await getUsage(accountId);
      setAccounts((prev) =>
        prev.map((a) =>
          a.id === accountId ? { ...a, usage, usageLoading: false } : a
        )
      );
    } catch (err) {
      console.error("Failed to refresh single usage:", err);
      setAccounts((prev) =>
        prev.map((a) =>
          a.id === accountId ? { ...a, usageLoading: false } : a
        )
      );
      throw err;
    }
  }, []);

  const warmupAccount = useCallback(async (accountId: string) => {
    try {
      await warmupAccountIpc(accountId);
    } catch (err) {
      console.error("Failed to warm up account:", err);
      throw err;
    }
  }, []);

  const warmupAllAccounts = useCallback(async () => {
    try {
      return await warmupAllAccountsIpc();
    } catch (err) {
      console.error("Failed to warm up all accounts:", err);
      throw err;
    }
  }, []);

  const switchAccount = useCallback(
    async (accountId: string) => {
      try {
        await switchAccountIpc(accountId);
        await loadAccounts(true); // Preserve usage data
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const deleteAccount = useCallback(
    async (accountId: string) => {
      try {
        await deleteAccountIpc(accountId);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const renameAccount = useCallback(
    async (accountId: string, newName: string) => {
      try {
        await renameAccountIpc(accountId, newName);
        await loadAccounts(true); // Preserve usage data
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const importFromFile = useCallback(
    async (path: string, name: string) => {
      try {
        await addAccountFromFile(path, name);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const deleteSelectedAccounts = useCallback(
    async (accountIds: string[]) => {
      try {
        await deleteAccountsBulk(accountIds);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const startOAuthLogin = useCallback(async (accountName: string) => {
    try {
      const info = await startLogin(accountName);
      return info;
    } catch (err) {
      throw err;
    }
  }, []);

  const completeOAuthLogin = useCallback(async () => {
    try {
      const account = await completeLogin();
      await loadAccounts();
      return account;
    } catch (err) {
      throw err;
    }
  }, [loadAccounts]);

  const importClaudeCredentials = useCallback(
    async (name: string) => {
      try {
        await importClaudeCredentialsIpc(name);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const importClaudeCredentialsFromPath = useCallback(
    async (name: string, path: string) => {
      try {
        await importClaudeCredentialsFromPathIpc(name, path);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const importGeminiCredentials = useCallback(
    async (name: string) => {
      try {
        await importGeminiCredentialsIpc(name);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const importGeminiCredentialsFromPath = useCallback(
    async (name: string, path: string) => {
      try {
        await importGeminiCredentialsFromPathIpc(name, path);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const addGeminiAccount = useCallback(
    async (name: string, cookie: string) => {
      try {
        await addSessionCookieAccount(name, cookie);
        await loadAccounts();
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const exportAccountsSlimText = useCallback(async () => {
    try {
      return await exportAccountsSlimTextIpc();
    } catch (err) {
      throw err;
    }
  }, []);

  const exportSelectedAccountsSlimText = useCallback(async (accountIds: string[]) => {
    try {
      return await exportSelectedAccountsSlimTextIpc(accountIds);
    } catch (err) {
      throw err;
    }
  }, []);

  const importAccountsSlimText = useCallback(
    async (payload: string) => {
      try {
        const summary = await importAccountsSlimTextIpc(payload);
        await loadAccounts();
        return summary;
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const exportAccountsFullEncryptedFile = useCallback(
    async (path: string, passphrase: string) => {
      try {
        await exportAccountsFullEncryptedFileIpc(path, passphrase);
      } catch (err) {
        throw err;
      }
    },
    []
  );

  const exportSelectedAccountsFullEncryptedFile = useCallback(
    async (path: string, passphrase: string, accountIds: string[]) => {
      try {
        await exportSelectedAccountsFullEncryptedFileIpc(path, passphrase, accountIds);
      } catch (err) {
        throw err;
      }
    },
    []
  );

  const importAccountsFullEncryptedFile = useCallback(
    async (path: string, passphrase: string) => {
      try {
        const summary = await importAccountsFullEncryptedFileIpc(path, passphrase);
        await loadAccounts();
        return summary;
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const cancelOAuthLogin = useCallback(async () => {
    try {
      await cancelLogin();
    } catch (err) {
      console.error("Failed to cancel login:", err);
    }
  }, []);

  const setAccountTags = useCallback(
    async (accountId: string, tags: string[]) => {
      try {
        await setAccountTagsIpc(accountId, tags);
        await loadAccounts(true);
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const setProviderHidden = useCallback(
    async (provider: Provider, hidden: boolean) => {
      try {
        await setProviderHiddenIpc(provider, hidden);
        await loadAccounts(true);
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const listAccountHistory = useCallback(async (accountId?: string, limit = 20) => {
    try {
      return await listAccountHistoryIpc(accountId, limit);
    } catch (err) {
      throw err;
    }
  }, []);

  const getProviderCapabilities = useCallback(async () => {
    try {
      return await getProviderCapabilitiesIpc();
    } catch (err) {
      throw err;
    }
  }, []);

  const getBestAccountRecommendation = useCallback(async (provider: Provider) => {
    try {
      return await getBestAccountRecommendationIpc(provider);
    } catch (err) {
      throw err;
    }
  }, []);

  const getDiagnostics = useCallback(async () => {
    try {
      return await getDiagnosticsIpc();
    } catch (err) {
      throw err;
    }
  }, []);

  const repairAccountSecret = useCallback(
    async (accountId: string) => {
      try {
        await repairAccountSecretIpc(accountId);
        await loadAccounts(true);
      } catch (err) {
        throw err;
      }
    },
    [loadAccounts]
  );

  const refreshSelectedUsage = useCallback(async (accountIds: string[]) => {
    try {
      const usageList = await refreshSelectedAccountsUsage(accountIds);
      setAccounts((prev) =>
        prev.map((account) => {
          const usage = usageList.find((item) => item.account_id === account.id);
          return usage ? { ...account, usage, usageLoading: false } : account;
        })
      );
      await loadAccounts(true);
      return usageList;
    } catch (err) {
      throw err;
    }
  }, [loadAccounts]);

  useEffect(() => {
    let cancelled = false;
    const unlistenFns: Array<() => void> = [];

    const bootstrapAccounts = async () => {
      try {
        await loadAccounts(false, true);
        await Promise.all([
          loadAppSettings(),
          getNotificationPermissionState(),
        ]);
        if (cancelled) return;
        void refreshUsage(false).catch((err) => {
          console.error("Failed to refresh usage during bootstrap:", err);
        });
      } catch (err) {
        console.error("Failed to bootstrap accounts:", err);
      }
    };

    void bootstrapAccounts();

    void listen("accounts-changed", () => {
      void loadAccounts(true).catch(() => {});
    }).then((dispose) => {
      if (cancelled) {
        dispose();
        return;
      }
      unlistenFns.push(dispose);
    });

    void listen<UsageInfo>("usage-updated", (event) => {
      setAccounts((prev) =>
        prev.map((account) =>
          account.id === event.payload.account_id
            ? { ...account, usage: event.payload, usageLoading: false }
            : account
        )
      );
    }).then((dispose) => {
      if (cancelled) {
        dispose();
        return;
      }
      unlistenFns.push(dispose);
    });

    void listen<AppSettings>("settings-changed", (event) => {
      setAppSettings(event.payload);
    }).then((dispose) => {
      if (cancelled) {
        dispose();
        return;
      }
      unlistenFns.push(dispose);
    });

    return () => {
      cancelled = true;
      unlistenFns.forEach((dispose) => dispose());
    };
  }, [getNotificationPermissionState, loadAccounts, loadAppSettings, refreshUsage]);

  return {
    accounts,
    appSettings,
    notificationPermission,
    loading,
    error,
    loadAccounts,
    refreshUsage,
    refreshSingleUsage,
    warmupAccount,
    warmupAllAccounts,
    switchAccount,
    deleteAccount,
    deleteAccountsBulk: deleteSelectedAccounts,
    renameAccount,
    importFromFile,
    importClaudeCredentials,
    importClaudeCredentialsFromPath,
    importGeminiCredentials,
    importGeminiCredentialsFromPath,
    addGeminiAccount,
    exportAccountsSlimText,
    exportSelectedAccountsSlimText,
    importAccountsSlimText,
    exportAccountsFullEncryptedFile,
    exportSelectedAccountsFullEncryptedFile,
    importAccountsFullEncryptedFile,
    setAccountTags,
    setProviderHidden,
    listAccountHistory,
    getProviderCapabilities,
    getBestAccountRecommendation,
    getDiagnostics,
    repairAccountSecret,
    refreshSelectedUsage,
    loadAppSettings,
    updateAppSettings,
    getNotificationPermissionState,
    requestNotificationPermission,
    sendTestNotification,
    startOAuthLogin,
    completeOAuthLogin,
    cancelOAuthLogin,
  };
}
