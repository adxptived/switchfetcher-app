import { AccountCard } from "./AccountCard";
import type { AccountWithUsage, AppSettings } from "../types";

type SortMode = "deadline_asc" | "deadline_desc" | "remaining_desc" | "remaining_asc";

interface AccountGridProps {
  loading: boolean;
  error: string | null;
  accounts: AccountWithUsage[];
  filteredAccounts: AccountWithUsage[];
  activeAccounts: AccountWithUsage[];
  otherAccounts: AccountWithUsage[];
  sortedOtherAccounts: AccountWithUsage[];
  selectedAccounts: AccountWithUsage[];
  selectedAccountIds: Set<string>;
  failedAccountIds: string[];
  appSettings: AppSettings | null;
  otherAccountsSort: SortMode;
  bulkMode: boolean;
  refreshSuccess: boolean;
  maskedAccounts: Set<string>;
  switchingId: string | null;
  warmingUpId: string | null;
  isWarmingAll: boolean;
  onOtherAccountsSortChange: (mode: SortMode) => void;
  onAddAccount: () => void;
  onToggleSelect: (accountId: string) => void;
  onSwitch: (accountId: string) => void;
  onWarmupAccount: (accountId: string, accountName: string) => Promise<void>;
  onDelete: (accountId: string) => void;
  onRefreshSingle: (accountId: string) => Promise<void>;
  onRepair: (account: AccountWithUsage) => Promise<void>;
  onRename: (accountId: string, newName: string) => Promise<void>;
  onUpdateTags: (account: AccountWithUsage) => void;
  onToggleMask: (accountId: string) => void;
  getSwitchDisabledReason: (account: AccountWithUsage) => string | null;
  onExportSelectedSlimText: () => void;
  onExportSelectedFullFile: () => void;
  onClearSelection: () => void;
  onRefreshFailed: () => void;
  onSelectFiltered: () => void;
  onRefreshSelected: () => void;
  onDeleteSelected: () => void;
}

export function AccountGrid(props: AccountGridProps) {
  return (
    <main className="max-w-6xl mx-auto px-6 py-8">
      {props.loading && props.accounts.length === 0 ? <div className="flex flex-col items-center justify-center py-20"><div className="animate-spin h-10 w-10 border-2 border-t-transparent rounded-full mb-4" style={{ borderColor: "var(--color-text-primary)", borderTopColor: "transparent" }} /><p style={{ color: "var(--color-text-secondary)" }}>Loading accounts...</p></div> : props.error ? <div className="text-center py-20"><div className="mb-2 text-red-600">Failed to load accounts</div><p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>{props.error}</p></div> : props.filteredAccounts.length === 0 && props.activeAccounts.length === 0 ? <div className="text-center py-20"><div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl" style={{ background: "var(--color-bg-muted)" }}><span className="text-3xl">👤</span></div><h2 className="mb-2 text-xl font-semibold" style={{ color: "var(--color-text-primary)" }}>No accounts for this filter</h2><p className="mb-6" style={{ color: "var(--color-text-secondary)" }}>Adjust provider/tag/search filters or add a new account</p><button onClick={props.onAddAccount} className="rounded-lg px-6 py-3 text-sm font-medium sf-btn-primary">Add Account</button></div> : <div className="space-y-6">
        {props.activeAccounts.length > 0 ? <section><h2 className="mb-4 text-sm font-medium uppercase tracking-wider" style={{ color: "var(--color-text-secondary)" }}>{props.activeAccounts.length === 1 ? "Active Account" : "Active Accounts"}</h2><div className="grid grid-cols-1 gap-4 md:grid-cols-2">{props.activeAccounts.map((account) => <AccountCard key={account.id} account={account} onSwitch={() => undefined} onWarmup={() => props.onWarmupAccount(account.id, account.name)} onDelete={() => props.onDelete(account.id)} onRefresh={() => props.onRefreshSingle(account.id)} onRepair={() => props.onRepair(account)} onRename={(newName) => props.onRename(account.id, newName)} onUpdateTags={account.load_state === "ready" ? () => props.onUpdateTags(account) : undefined} onToggleSelect={() => props.onToggleSelect(account.id)} selectionMode={props.bulkMode} selected={props.selectedAccountIds.has(account.id)} switching={props.switchingId === account.id} switchDisabled={Boolean(props.getSwitchDisabledReason(account))} switchDisabledReason={props.getSwitchDisabledReason(account) ?? undefined} warmingUp={props.isWarmingAll || props.warmingUpId === account.id} masked={props.maskedAccounts.has(account.id)} onToggleMask={() => props.onToggleMask(account.id)} use24hTime={props.appSettings?.use_24h_time ?? false} />)}</div></section> : null}
        {props.selectedAccounts.length > 0 ? <section className="rounded-2xl border border-amber-200 bg-amber-50 p-4"><div className="flex flex-wrap items-center gap-2 justify-between"><div className="text-sm text-amber-900 font-medium">{props.selectedAccounts.length} account(s) selected</div><div className="flex flex-wrap gap-2"><button onClick={props.onRefreshSelected} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Refresh Selected</button><button onClick={props.onDeleteSelected} className="px-3 py-2 rounded-lg bg-white border border-red-200 text-sm text-red-600">Delete Selected</button><button onClick={props.onExportSelectedSlimText} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Export Selected Slim</button><button onClick={props.onExportSelectedFullFile} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Export Selected Full</button><button onClick={props.onClearSelection} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Clear Selection</button></div></div></section> : null}
        <section className="rounded-2xl p-4 sf-panel"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Batch actions</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Failed refreshes: {props.failedAccountIds.length} • Hidden filtered by toggle</div></div><div className="flex flex-wrap gap-2"><button onClick={props.onRefreshFailed} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Refresh Failed Only</button><button onClick={props.onSelectFiltered} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Select Filtered</button></div></div></section>
        {props.otherAccounts.length > 0 ? <section><div className="mb-4 flex items-center justify-between gap-3"><h2 className="text-sm font-medium uppercase tracking-wider" style={{ color: "var(--color-text-secondary)" }}>Other Accounts ({props.otherAccounts.length})</h2><select value={props.otherAccountsSort} onChange={(event) => props.onOtherAccountsSortChange(event.target.value as SortMode)} className="appearance-none rounded-xl px-3 py-2 text-sm font-medium sf-input"><option value="deadline_asc">Reset: earliest to latest</option><option value="deadline_desc">Reset: latest to earliest</option><option value="remaining_desc">% remaining: highest to lowest</option><option value="remaining_asc">% remaining: lowest to highest</option></select></div><div className="grid grid-cols-1 gap-4 md:grid-cols-2">{props.sortedOtherAccounts.map((account) => <AccountCard key={account.id} account={account} onSwitch={() => props.onSwitch(account.id)} onWarmup={() => props.onWarmupAccount(account.id, account.name)} onDelete={() => props.onDelete(account.id)} onRefresh={() => props.onRefreshSingle(account.id)} onRepair={() => props.onRepair(account)} onRename={(newName) => props.onRename(account.id, newName)} onUpdateTags={account.load_state === "ready" ? () => props.onUpdateTags(account) : undefined} onToggleSelect={() => props.onToggleSelect(account.id)} selectionMode={props.bulkMode} selected={props.selectedAccountIds.has(account.id)} switching={props.switchingId === account.id} switchDisabled={Boolean(props.getSwitchDisabledReason(account))} switchDisabledReason={props.getSwitchDisabledReason(account) ?? undefined} warmingUp={props.isWarmingAll || props.warmingUpId === account.id} masked={props.maskedAccounts.has(account.id)} onToggleMask={() => props.onToggleMask(account.id)} use24hTime={props.appSettings?.use_24h_time ?? false} />)}</div></section> : null}
      </div>}
      {props.refreshSuccess ? <div className="fixed bottom-6 left-1/2 -translate-x-1/2 rounded-lg bg-green-600 px-4 py-3 text-sm text-white shadow-lg">Usage refreshed successfully</div> : null}
    </main>
  );
}
