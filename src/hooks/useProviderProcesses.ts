import { useCallback, useEffect, useState } from "react";
import {
  checkClaudeProcesses,
  checkCodexProcesses,
  checkGeminiProcesses,
} from "../ipc";
import type {
  ClaudeProcessInfo,
  CodexProcessInfo,
  GeminiProcessInfo,
} from "../types";

export function useProviderProcesses() {
  const [codexProcessInfo, setCodexProcessInfo] = useState<CodexProcessInfo | null>(null);
  const [claudeProcessInfo, setClaudeProcessInfo] = useState<ClaudeProcessInfo | null>(null);
  const [geminiProcessInfo, setGeminiProcessInfo] = useState<GeminiProcessInfo | null>(null);

  const refreshCodexProcesses = useCallback(async () => {
    try {
      const info = await checkCodexProcesses();
      setCodexProcessInfo(info);
      return info;
    } catch {
      setCodexProcessInfo(null);
      return null;
    }
  }, []);

  const refreshClaudeProcesses = useCallback(async () => {
    try {
      setClaudeProcessInfo(await checkClaudeProcesses());
    } catch {
      setClaudeProcessInfo(null);
    }
  }, []);

  const refreshGeminiProcesses = useCallback(async () => {
    try {
      setGeminiProcessInfo(await checkGeminiProcesses());
    } catch {
      setGeminiProcessInfo(null);
    }
  }, []);

  const refreshAllProcesses = useCallback(async () => {
    await Promise.all([
      refreshCodexProcesses(),
      refreshClaudeProcesses(),
      refreshGeminiProcesses(),
    ]);
  }, [refreshClaudeProcesses, refreshCodexProcesses, refreshGeminiProcesses]);

  useEffect(() => {
    void refreshAllProcesses();
    const interval = setInterval(() => {
      void refreshAllProcesses();
    }, 3000);
    return () => clearInterval(interval);
  }, [refreshAllProcesses]);

  return {
    codexProcessInfo,
    claudeProcessInfo,
    geminiProcessInfo,
    refreshCodexProcesses,
  };
}
