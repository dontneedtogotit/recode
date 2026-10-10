import { Component, ErrorInfo, ReactNode } from 'react';

type Props = {
  children: ReactNode;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
};

type State = {
  hasError: boolean;
  error: Error | null;
};

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }
      return (
        <div className="view-error-state" role="alert">
          <div className="view-error-icon" aria-hidden="true">!</div>
          <h2>Something went wrong</h2>
          <p>An unexpected error occurred in this view. The rest of the app is still available.</p>
          <details>
            <summary>Show error details</summary>
            <pre>{this.state.error?.message}{this.state.error?.stack ? `\n\n${this.state.error.stack}` : ''}</pre>
          </details>
          <button
            type="button"
            onClick={() => this.setState({ hasError: false, error: null })}
            className="secondary"
          >
            Try again
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}