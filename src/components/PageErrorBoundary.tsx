import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface Props {
  /** Change de page : on réessaie l'affichage. */
  resetKey: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** Une page qui plante affiche l'erreur au lieu de laisser la fenêtre vide. */
export class PageErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("page en erreur", error, info.componentStack);
  }

  componentDidUpdate(prev: Props) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-xs">
        <p className="text-sm font-semibold">Cette page a rencontré une erreur.</p>
        <pre className="max-w-full overflow-auto rounded-md bg-muted p-3 text-[11px] whitespace-pre-wrap">
          {this.state.error.message}
        </pre>
        <p className="text-muted-foreground">
          Les autres pages restent utilisables depuis le menu. Envoie-moi ce message si ça se
          reproduit.
        </p>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            this.setState({ error: null });
          }}
        >
          Réessayer
        </Button>
      </div>
    );
  }
}
