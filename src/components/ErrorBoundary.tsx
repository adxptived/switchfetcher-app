import { Component, type ReactNode } from "react";

interface ErrorBoundaryProps {
  fallbackTitle?: string;
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  componentDidCatch(error: unknown) {
    console.error("ErrorBoundary caught an error", error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="rounded-2xl border border-red-200 bg-red-50 p-4 text-sm text-red-600">
          <div className="font-semibold">
            {this.props.fallbackTitle ?? "Section failed to render"}
          </div>
          <div className="mt-1 text-red-500">
            Reload the view or reopen this panel.
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
