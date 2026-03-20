import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipcMocks = vi.hoisted(() => ({
  checkCodexProcesses: vi.fn(),
  checkClaudeProcesses: vi.fn(),
  checkGeminiProcesses: vi.fn(),
}));

vi.mock("../ipc", () => ipcMocks);

import { useProviderProcesses } from "./useProviderProcesses";

describe("useProviderProcesses", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    ipcMocks.checkCodexProcesses.mockResolvedValue(null);
    ipcMocks.checkClaudeProcesses.mockResolvedValue(null);
    ipcMocks.checkGeminiProcesses.mockResolvedValue(null);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("polls all three providers on mount", async () => {
    renderHook(() => useProviderProcesses());

    await waitFor(() => {
      expect(ipcMocks.checkCodexProcesses).toHaveBeenCalledTimes(1);
      expect(ipcMocks.checkClaudeProcesses).toHaveBeenCalledTimes(1);
      expect(ipcMocks.checkGeminiProcesses).toHaveBeenCalledTimes(1);
    });
  });

  it("creates exactly one interval and cleans it up on unmount", () => {
    vi.useFakeTimers();
    const setIntervalSpy = vi.spyOn(globalThis, "setInterval");
    const clearIntervalSpy = vi.spyOn(globalThis, "clearInterval");

    const { unmount } = renderHook(() => useProviderProcesses());

    expect(setIntervalSpy).toHaveBeenCalledTimes(1);

    unmount();

    expect(clearIntervalSpy).toHaveBeenCalledTimes(1);
  });
});
