import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { PullActingAs } from "./parts";

/**
 * Text box + "with ● account" + submit, for comments and replies. `onSubmit` resolves to `true`
 * when the text was posted (the box is then cleared).
 */
export function Composer({
  actor,
  onActor,
  onSubmit,
  placeholder,
  submitLabel,
  onCancel,
  autoFocus,
  allowEmpty = false,
  extra,
}: {
  actor: string;
  onActor: (accountId: string) => void;
  onSubmit: (text: string) => Promise<boolean>;
  placeholder: string;
  submitLabel: string;
  onCancel?: () => void;
  autoFocus?: boolean;
  allowEmpty?: boolean;
  /** Controls shown above the buttons (review verdict…). */
  extra?: React.ReactNode;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    if (busy || (!allowEmpty && !text.trim())) return;
    setBusy(true);
    const ok = await onSubmit(text);
    setBusy(false);
    if (ok) setText("");
  };
  return (
    <div className="flex flex-col gap-1.5 rounded-md border bg-background p-2">
      <textarea
        value={text}
        autoFocus={autoFocus}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            void submit();
          }
        }}
        placeholder={placeholder}
        rows={3}
        className="min-h-16 w-full resize-y rounded border bg-background px-2 py-1 font-sans text-xs focus:border-primary focus:outline-none"
      />
      {extra}
      <div className="flex items-center gap-2">
        <PullActingAs accountId={actor} onChange={onActor} disabled={busy} />
        <span className="flex-1" />
        {onCancel && (
          <Button size="sm" variant="ghost" onClick={onCancel} disabled={busy}>
            {t("pulls.cancel")}
          </Button>
        )}
        <Button
          size="sm"
          onClick={() => void submit()}
          disabled={busy || (!allowEmpty && !text.trim())}
        >
          {submitLabel}
        </Button>
      </div>
    </div>
  );
}
