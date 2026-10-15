import { Component, type ReactNode } from "react";
import { TriangleAlert } from "lucide-react";
import i18n from "i18next";
import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  /** Closes the failed view (the boundary resets when its `key` changes). */
  onClose: () => void;
  /** A new value (another file opened) clears the error. */
  resetKey?: string;
}

interface State {
  error: Error | null;
}

/** Keeps a crashing center view (diff, editor, blame…) from blanking the whole window. */
export class ViewErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidUpdate(prev: Props) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  componentDidCatch(error: Error) {
    console.error("view crashed", error);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        role="alert"
        className="flex min-w-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center"
      >
        <TriangleAlert className="size-6 text-warning" />
        <p className="text-sm font-medium">{i18n.t("viewError.title")}</p>
        <p className="max-w-md text-xs text-muted-foreground">{i18n.t("viewError.body")}</p>
        <pre className="max-w-md truncate text-[11px] text-muted-foreground" title={error.message}>
          {error.message}
        </pre>
        <Button size="sm" variant="outline" onClick={this.props.onClose}>
          {i18n.t("common.close")}
        </Button>
      </div>
    );
  }
}
