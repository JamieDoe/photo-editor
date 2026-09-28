import { Component, type ErrorInfo, type ReactNode } from "react";
import { ErrorBanner } from "../components/ErrorBanner";
import { report, type AppError } from "./errors";

interface State {
  error: AppError | null;
}

/**
 * Last line of defence for render errors: logs the error locally and shows a
 * recoverable screen instead of a blank window. The engine and its open images are
 * unaffected; reloading the UI reconnects to them.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(): State {
    return { error: { message: "The window ran into a problem and needs to reload.", reference: null } };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    const withStack = Object.assign(new Error(error.message), {
      name: error.name,
      stack: `${error.stack ?? ""}\nComponent stack:${info.componentStack ?? ""}`,
    });
    void report("render", withStack).then((reference) => {
      this.setState((s) => (s.error ? { error: { ...s.error, reference } } : s));
    });
  }

  override render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="fatal">
        <ErrorBanner error={this.state.error} onDismiss={() => window.location.reload()} />
        <button className="fatal-reload" onClick={() => window.location.reload()}>
          Reload window
        </button>
      </div>
    );
  }
}
