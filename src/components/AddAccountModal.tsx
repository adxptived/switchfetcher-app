import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import type { Provider } from "../types";
import { Badge } from "./Badge";

interface AddAccountModalProps {
  isOpen: boolean;
  onClose: () => void;
  onImportFile: (path: string, name: string) => Promise<void>;
  onImportClaude: (name: string) => Promise<void>;
  onImportClaudeFromPath: (name: string, path: string) => Promise<void>;
  onImportGemini: (name: string) => Promise<void>;
  onImportGeminiFromPath: (name: string, path: string) => Promise<void>;
  onAddGemini: (name: string, cookie: string) => Promise<void>;
  onStartOAuth: (name: string) => Promise<{ auth_url: string }>;
  onCompleteOAuth: () => Promise<unknown>;
  onCancelOAuth: () => Promise<void>;
}

type Tab = "oauth" | "import" | "claude" | "gemini_oauth" | "session";

function formatOAuthErrorMessage(message: string): string {
  const normalized = message.trim();

  if (
    normalized.includes("unknown_error") ||
    normalized.includes("failed temporarily")
  ) {
    return [
      "OpenAI login failed temporarily.",
      "Try again in a few seconds.",
      "If it keeps happening, clear cookies for chatgpt.com/auth.openai.com, disable VPN or proxy, or retry in an incognito window.",
      normalized.includes("Provider details:")
        ? normalized.slice(normalized.indexOf("Provider details:"))
        : normalized,
    ].join(" ");
  }

  return normalized;
}

const providerConfig: Record<
  Provider,
  { label: string; accent: string; helperText: string; browserUrl?: string }
> = {
  codex: {
    label: "Codex",
    accent: "bg-slate-900 text-white",
    helperText: "Use ChatGPT OAuth or import an existing Codex auth.json file.",
  },
  claude: {
    label: "Claude",
    accent: "bg-orange-500 text-white",
    helperText: "Import OAuth credentials from ~/.claude/.credentials.json.",
  },
  gemini: {
    label: "Gemini",
    accent: "bg-blue-600 text-white",
    browserUrl: "https://gemini.google.com/app",
    helperText:
      "Open DevTools → Application → Cookies and copy __Secure-1PSID plus __Secure-1PSIDTS.",
  },
};

export function AddAccountModal({
  isOpen,
  onClose,
  onImportFile,
  onImportClaude,
  onImportClaudeFromPath,
  onImportGemini,
  onImportGeminiFromPath,
  onAddGemini,
  onStartOAuth,
  onCompleteOAuth,
  onCancelOAuth,
}: AddAccountModalProps) {
  const [provider, setProvider] = useState<Provider>("codex");
  const [activeTab, setActiveTab] = useState<Tab>("oauth");
  const [name, setName] = useState("");
  const [filePath, setFilePath] = useState("");
  const [claudeFilePath, setClaudeFilePath] = useState("");
  const [geminiFilePath, setGeminiFilePath] = useState("");
  const [cookie, setCookie] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [oauthPending, setOauthPending] = useState(false);
  const [authUrl, setAuthUrl] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    setActiveTab(
      provider === "codex"
        ? "oauth"
        : provider === "claude"
          ? "claude"
          : "gemini_oauth"
    );
    setError(null);
    setFilePath("");
    setClaudeFilePath("");
    setGeminiFilePath("");
    setCookie("");
  }, [provider]);

  const resetForm = () => {
    setProvider("codex");
    setActiveTab("oauth");
    setName("");
    setFilePath("");
    setClaudeFilePath("");
    setGeminiFilePath("");
    setCookie("");
    setError(null);
    setLoading(false);
    setOauthPending(false);
    setAuthUrl("");
    setCopied(false);
  };

  const currentProvider = providerConfig[provider];
  const isPrimaryDisabled = loading || (activeTab === "oauth" && oauthPending);
  const tabs: Tab[] =
    provider === "codex"
      ? ["oauth", "import"]
      : provider === "claude"
        ? ["claude"]
        : ["gemini_oauth", "session"];

  const primaryLabel = useMemo(() => {
    if (loading) return "Adding...";
    if (activeTab === "oauth") return "Generate Login Link";
    if (activeTab === "import") return "Import";
    if (activeTab === "claude") return "Import Claude Credentials";
    if (activeTab === "gemini_oauth") return "Import Gemini OAuth";
    return "Add Gemini Account";
  }, [activeTab, loading]);

  const handleClose = () => {
    if (oauthPending) {
      void onCancelOAuth().catch(() => {});
    }
    resetForm();
    onClose();
  };

  const handleProviderChange = (nextProvider: Provider) => {
    if (oauthPending) {
      void onCancelOAuth().catch(() => {});
      setOauthPending(false);
    }
    setProvider(nextProvider);
  };

  const handleOAuthLogin = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const info = await onStartOAuth(name.trim());
      setAuthUrl(info.auth_url);
      setOauthPending(true);
      setLoading(false);
      await onCompleteOAuth();
      handleClose();
    } catch (err) {
      setError(formatOAuthErrorMessage(err instanceof Error ? err.message : String(err)));
      setLoading(false);
      setOauthPending(false);
    }
  };

  const handleSelectFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Select auth.json file",
      });

      if (selected && !Array.isArray(selected)) {
        setFilePath(selected);
      }
    } catch (err) {
      console.error("Failed to open file dialog:", err);
    }
  };

  const handleImportFile = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }
    if (!filePath.trim()) {
      setError("Please select an auth.json file");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      await onImportFile(filePath.trim(), name.trim());
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
    }
  };

  const handleImportClaude = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      if (claudeFilePath.trim()) {
        await onImportClaudeFromPath(name.trim(), claudeFilePath.trim());
      } else {
        await onImportClaude(name.trim());
      }
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
    }
  };

  const handleSelectClaudeFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Select Claude .credentials.json file",
      });

      if (selected && !Array.isArray(selected)) {
        setClaudeFilePath(selected);
      }
    } catch (err) {
      console.error("Failed to open Claude credentials file dialog:", err);
    }
  };

  const handleImportGemini = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      if (geminiFilePath.trim()) {
        await onImportGeminiFromPath(name.trim(), geminiFilePath.trim());
      } else {
        await onImportGemini(name.trim());
      }
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
    }
  };

  const handleSelectGeminiFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Select Gemini oauth_creds.json file",
      });

      if (selected && !Array.isArray(selected)) {
        setGeminiFilePath(selected);
      }
    } catch (err) {
      console.error("Failed to open Gemini credentials file dialog:", err);
    }
  };

  const handleAddGemini = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }
    if (!cookie.trim()) {
      setError("Paste the browser cookie first");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      await onAddGemini(name.trim(), cookie.trim());
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay">
      <div className="mx-4 w-full max-w-xl rounded-2xl sf-panel">
        <div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}>
          <div>
            <h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Add Account</h2>
            <p className="text-xs" style={{ color: "var(--color-text-secondary)" }}>
              Connect Codex, Claude, or Gemini to Switchfetcher.
            </p>
          </div>
          <button
            onClick={handleClose}
            className="transition-colors"
            style={{ color: "var(--color-text-muted)" }}
          >
            ✕
          </button>
        </div>

        <div className="border-b p-5 pb-4" style={{ borderColor: "var(--color-border)" }}>
          <div className="grid grid-cols-3 gap-2 rounded-xl p-1" style={{ background: "var(--color-btn-secondary-bg)" }}>
            {(["codex", "claude", "gemini"] as Provider[]).map((option) => (
              <button
                key={option}
                onClick={() => handleProviderChange(option)}
                className={`rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                  provider === option
                    ? `${providerConfig[option].accent} shadow-sm`
                    : ""
                }`}
                style={provider === option ? undefined : { color: "var(--color-text-secondary)" }}
              >
                <span className="inline-flex items-center gap-2">
                  <span>{providerConfig[option].label}</span>
                  {option === "gemini" ? (
                    <Badge
                      variant="experimental"
                      label="BETA"
                      title="Gemini support is experimental. Usage may be unavailable."
                    />
                  ) : null}
                </span>
              </button>
            ))}
          </div>
        </div>

        <div className="flex border-b" style={{ borderColor: "var(--color-border)" }}>
          {tabs.map((tab) => (
            <button
              key={tab}
              onClick={() => {
                if (tab === "import" && oauthPending) {
                  void onCancelOAuth().catch(() => {});
                  setOauthPending(false);
                  setLoading(false);
                }
                setActiveTab(tab);
                setError(null);
              }}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
                activeTab === tab
                  ? "border-b-2 -mb-px"
                  : ""
              }`}
              style={
                activeTab === tab
                  ? { borderColor: "var(--color-text-primary)", color: "var(--color-text-primary)" }
                  : { color: "var(--color-text-muted)" }
              }
            >
              {tab === "oauth"
                ? "ChatGPT Login"
                : tab === "import"
                  ? "Import File"
                  : tab === "claude"
                    ? "Claude OAuth"
                    : tab === "gemini_oauth"
                      ? "Gemini OAuth"
                      : "Session Cookie"}
            </button>
          ))}
        </div>

        <div className="space-y-4 p-5">
          <div>
            <label className="mb-2 block text-sm font-medium text-[var(--color-text-primary)]">
              Account Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={`e.g., ${currentProvider.label} Work`}
              className="w-full rounded-lg px-4 py-2.5 transition-colors sf-input"
            />
          </div>

          {activeTab === "oauth" && (
            <div className="text-sm text-[var(--color-text-secondary)]">
              {oauthPending ? (
                <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-muted)] p-4 text-center">
                  <div className="mx-auto mb-3 h-8 w-8 animate-spin rounded-full border-2 border-[var(--color-text-primary)] border-t-transparent" />
                  <p className="mb-2 font-medium text-[var(--color-text-primary)]">
                    Waiting for browser login...
                  </p>
                  <p className="mb-4 text-xs text-[var(--color-text-secondary)]">
                    Open the generated link in your browser to complete the Codex login.
                  </p>
                  <p className="mb-4 text-xs text-[var(--color-text-muted)]">
                    If the browser shows OpenAI `unknown_error`, close that tab and generate a new
                    login link here.
                  </p>
                  <div className="mb-2 flex items-center gap-2 rounded-lg p-2 sf-input">
                    <input
                      type="text"
                      readOnly
                      value={authUrl}
                      className="flex-1 truncate bg-transparent text-xs focus:outline-none"
                      style={{ color: "var(--color-text-secondary)" }}
                    />
                    <button
                      onClick={() => {
                        navigator.clipboard.writeText(authUrl);
                        setCopied(true);
                        setTimeout(() => setCopied(false), 2000);
                      }}
                      className={`shrink-0 rounded border px-3 py-1.5 text-xs font-medium transition-colors ${
                        copied
                          ? "border-green-200 bg-green-50 text-green-700"
                          : "border-[var(--color-border)] bg-[var(--color-bg-card)] text-[var(--color-text-primary)]"
                      }`}
                    >
                      {copied ? "Copied!" : "Copy"}
                    </button>
                    <button
                      onClick={() => openUrl(authUrl)}
                      className="shrink-0 rounded border px-3 py-1.5 text-xs font-medium transition-colors sf-btn-primary"
                      style={{ borderColor: "var(--color-btn-primary-bg)" }}
                    >
                      Open
                    </button>
                  </div>
                </div>
              ) : (
                <div className="rounded-xl border border-gray-200 bg-gray-50 p-4">
                  Click the button below to generate a browser login link for your Codex account.
                </div>
              )}
            </div>
          )}

          {activeTab === "import" && (
            <div>
              <label className="mb-2 block text-sm font-medium text-[var(--color-text-primary)]">
                Select auth.json file
              </label>
              <div className="flex gap-2">
                <div className="flex-1 truncate rounded-lg px-4 py-2.5 text-sm sf-input" style={{ background: "var(--color-bg-muted)", color: "var(--color-text-secondary)" }}>
                  {filePath || "No file selected"}
                </div>
                <button
                  onClick={handleSelectFile}
                  className="whitespace-nowrap rounded-lg px-4 py-2.5 text-sm font-medium transition-colors sf-btn-secondary"
                >
                  Browse...
                </button>
              </div>
              <p className="mt-2 text-xs text-[var(--color-text-muted)]">
                Import credentials from an existing Codex `auth.json` file.
              </p>
            </div>
          )}

          {activeTab === "claude" && (
            <div className="space-y-4">
              <div className="rounded-xl border border-orange-100 bg-orange-50 p-4">
                <p className="text-sm font-medium text-orange-900">
                  Import saved Claude CLI credentials
                </p>
                <p className="mt-1 text-xs text-orange-800">
                  By default Switchfetcher reads `~/.claude/.credentials.json`, but you can also
                  point it to a custom credentials file.
                </p>
              </div>
              <div>
                <label className="mb-2 block text-sm font-medium text-[var(--color-text-primary)]">
                  Optional credentials file
                </label>
                <div className="flex gap-2">
                  <div className="flex-1 truncate rounded-lg px-4 py-2.5 text-sm sf-input" style={{ background: "var(--color-bg-muted)", color: "var(--color-text-secondary)" }}>
                    {claudeFilePath || "Default path: ~/.claude/.credentials.json"}
                  </div>
                  <button
                    onClick={handleSelectClaudeFile}
                    className="whitespace-nowrap rounded-lg px-4 py-2.5 text-sm font-medium transition-colors sf-btn-secondary"
                  >
                    Browse...
                  </button>
                </div>
                <p className="mt-2 text-xs text-[var(--color-text-muted)]">
                  Leave this empty to import from the default Claude CLI location.
                </p>
              </div>
              <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-muted)] p-4 text-xs text-[var(--color-text-secondary)]">
                Make sure `claude login` has already created the credentials file on this machine.
              </div>
            </div>
          )}

          {activeTab === "gemini_oauth" && (
            <div className="space-y-4">
              <div className="rounded-xl border border-blue-100 bg-blue-50 p-4">
                <p className="text-sm font-medium text-blue-900">
                  Import saved Gemini CLI OAuth credentials
                </p>
                <p className="mt-1 text-xs text-blue-800">
                  By default Switchfetcher reads `~/.gemini/oauth_creds.json`, but you can also
                  point it to a custom credentials file.
                </p>
              </div>
              <div>
                <label className="mb-2 block text-sm font-medium text-[var(--color-text-primary)]">
                  Optional credentials file
                </label>
                <div className="flex gap-2">
                  <div className="flex-1 truncate rounded-lg px-4 py-2.5 text-sm sf-input" style={{ background: "var(--color-bg-muted)", color: "var(--color-text-secondary)" }}>
                    {geminiFilePath || "Default path: ~/.gemini/oauth_creds.json"}
                  </div>
                  <button
                    onClick={handleSelectGeminiFile}
                    className="whitespace-nowrap rounded-lg px-4 py-2.5 text-sm font-medium transition-colors sf-btn-secondary"
                  >
                    Browse...
                  </button>
                </div>
                <p className="mt-2 text-xs text-[var(--color-text-muted)]">
                  Leave this empty to import from the default Gemini CLI location.
                </p>
              </div>
              <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-muted)] p-4 text-xs text-[var(--color-text-secondary)]">
                Make sure Gemini CLI has already created `oauth_creds.json` on this machine.
              </div>
            </div>
          )}

          {activeTab === "session" && (
            <div className="space-y-4">
              <div className="rounded-xl border border-gray-200 bg-gray-50 p-4">
                <div className="mb-2 flex items-center justify-between gap-3">
                  <div>
                    <p className="text-sm font-medium text-[var(--color-text-primary)]">
                      Gemini session cookie
                    </p>
                    <p className="mt-1 text-xs text-[var(--color-text-secondary)]">{currentProvider.helperText}</p>
                  </div>
                  {currentProvider.browserUrl && (
                    <button
                      onClick={() => openUrl(currentProvider.browserUrl!)}
                      className="shrink-0 rounded-lg border px-3 py-2 text-xs font-medium text-white transition-colors sf-btn-primary"
                      style={{ borderColor: "var(--color-btn-primary-bg)" }}
                    >
                      Open {currentProvider.label}
                    </button>
                  )}
                </div>
                <ol className="list-decimal space-y-1 pl-4 text-xs text-[var(--color-text-secondary)]">
                  <li>Open Gemini and complete sign-in in your normal browser.</li>
                  <li>Open DevTools, then Application → Cookies.</li>
                  <li>Copy `__Secure-1PSID` and `__Secure-1PSIDTS`, or paste the whole Cookie header.</li>
                </ol>
              </div>

              <div>
                <label className="mb-2 block text-sm font-medium text-[var(--color-text-primary)]">
                  Session Cookie
                </label>
                <textarea
                  value={cookie}
                  onChange={(e) => setCookie(e.target.value)}
                  placeholder="__Secure-1PSID=...; __Secure-1PSIDTS=..."
                  className="h-40 w-full rounded-lg px-4 py-3 font-mono text-sm transition-colors sf-input"
                />
                <p className="mt-2 text-xs text-[var(--color-text-muted)]">
                  `Cookie:` prefix is allowed. Switchfetcher will normalize it before storing.
                </p>
              </div>
            </div>
          )}

          {error && (
            <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-600">
              {error}
            </div>
          )}
        </div>

        <div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}>
          <button
            onClick={handleClose}
            className="flex-1 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors sf-btn-secondary"
          >
            Cancel
          </button>
          <button
            onClick={
              activeTab === "oauth"
                ? handleOAuthLogin
                : activeTab === "import"
                  ? handleImportFile
                  : activeTab === "claude"
                    ? handleImportClaude
                    : activeTab === "gemini_oauth"
                      ? handleImportGemini
                    : handleAddGemini
            }
            disabled={isPrimaryDisabled}
            className="flex-1 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors sf-btn-primary disabled:opacity-50"
          >
            {primaryLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
