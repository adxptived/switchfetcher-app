import type { RefObject } from "react";
import type { CodexProcessInfo } from "../types";
import { Badge } from "./Badge";
import appIcon from "../assets/icon.png";

interface HeaderProps {
  theme: "light" | "dark";
  appVersion: string;
  processInfo: CodexProcessInfo | null;
  hasRunningProcesses: boolean;
  isActionsMenuOpen: boolean;
  actionsMenuRef: RefObject<HTMLDivElement | null>;
  isRefreshing: boolean;
  isWarmingAll: boolean;
  onThemeToggle: () => void;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
  onOpenDiagnostics: () => void;
  onOpenRecommendationPicker: () => void;
  onRefresh: () => void;
  onWarmupAll: () => void;
  onToggleActionsMenu: () => void;
  onOpenAddModal: () => void;
  onExportSlimText: () => void;
  onOpenImportSlimText: () => void;
  onExportFullFile: () => void;
  onImportFullFile: () => void;
}

function MoonIcon() {
  return <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z" /></svg>;
}

function SunIcon() {
  return <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4" strokeWidth={1.8} /><path strokeLinecap="round" strokeWidth={1.8} d="M12 2.5v2.2M12 19.3v2.2M21.5 12h-2.2M4.7 12H2.5M18.7 5.3l-1.6 1.6M6.9 17.1l-1.6 1.6M18.7 18.7l-1.6-1.6M6.9 6.9L5.3 5.3" /></svg>;
}

export function Header(props: HeaderProps) {
  return (
    <header className="sticky top-0 z-40 border-b" style={{ background: "var(--color-bg-header)", borderColor: "var(--color-border)" }}>
      <div className="max-w-6xl mx-auto px-6 py-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex items-center gap-3">
            <img src={appIcon} alt="Switchfetcher" className="h-10 w-10 rounded-xl" />
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <h1 className="text-xl font-bold tracking-tight" style={{ color: "var(--color-text-primary)" }}>Switchfetcher</h1>
                {props.processInfo ? props.hasRunningProcesses ? <Badge variant="status" status="warning" label={`${props.processInfo.count} Codex running`} /> : <Badge variant="status" status="healthy" label="0 Codex running" /> : null}
              </div>
              <p className="text-xs" style={{ color: "var(--color-text-secondary)" }}>{props.appVersion ? `v${props.appVersion}` : "Loading version..."}</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button onClick={props.onOpenRecommendationPicker} className="h-10 px-4 py-2 text-sm font-medium rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white">Switch To Best</button>
            <button onClick={props.onRefresh} disabled={props.isRefreshing} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary disabled:opacity-50">{props.isRefreshing ? "Refreshing..." : "Refresh All"}</button>
            <button onClick={props.onWarmupAll} disabled={props.isWarmingAll} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary disabled:opacity-50">{props.isWarmingAll ? "Warming..." : "Warm-up All Codex"}</button>
            <div className="relative" ref={props.actionsMenuRef}>
              <button onClick={props.onToggleActionsMenu} className="h-10 px-4 py-2 text-sm font-medium rounded-lg bg-gray-900 hover:bg-gray-800 text-white">Account ▾</button>
              {props.isActionsMenuOpen ? <div className="absolute right-0 mt-2 z-50 w-64 rounded-xl p-2 sf-panel">
                <button onClick={props.onOpenAddModal} className="w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">+ Add Account</button>
                <button onClick={props.onExportSlimText} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">Export Slim Text</button>
                <button onClick={props.onOpenImportSlimText} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">Import Slim Text</button>
                <button onClick={props.onExportFullFile} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">Export Full Encrypted File</button>
                <button onClick={props.onImportFullFile} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">Import Full Encrypted File</button>
              </div> : null}
            </div>
            <button onClick={props.onOpenSettings} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">Settings</button>
            <button onClick={props.onOpenHistory} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">History</button>
            <button onClick={props.onOpenDiagnostics} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">Diagnostics</button>
            <button onClick={props.onThemeToggle} className="flex h-10 w-10 items-center justify-center rounded-lg sf-btn-secondary" title={props.theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}>{props.theme === "dark" ? <SunIcon /> : <MoonIcon />}</button>
          </div>
        </div>
      </div>
    </header>
  );
}
