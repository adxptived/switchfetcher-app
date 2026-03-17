import { useEffect } from "react";

interface KeyboardShortcutsOptions {
  onRefreshAll: () => void;
  onEscape: () => void;
}

export function useKeyboardShortcuts({
  onRefreshAll,
  onEscape,
}: KeyboardShortcutsOptions) {
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const meta = event.ctrlKey || event.metaKey;
      if (meta && event.key.toLowerCase() === "r") {
        event.preventDefault();
        onRefreshAll();
        return;
      }
      if (event.key === "Escape") {
        onEscape();
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onEscape, onRefreshAll]);
}
