import { useRef, useState } from "react";
import { LoaderCircle, Sparkles, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { isAiError, newRequestId, sendsCodeAway, serviceName } from "@/features/ai/model";
import { useAiConfig } from "@/features/ai/queries";
import { openAiSettings } from "@/features/ai/store";
import { aiErrorText } from "@/features/ai/suggest";
import { commands } from "@/ipc";
import { useSettings } from "@/stores/settings";

interface Props {
  id: string;
  repoId: number;
  /** Repository name typed in the dialog (the model reads it too). */
  name: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  inputClassName?: string;
}

/**
 * "Description" field of Publish with an AI button next to it: asks the AI service for one
 * sentence about the project (from its name, README, manifest and file types, never its code).
 * Text already typed is kept for one "Hoàn tác". Without AI set up, the button opens the AI
 * settings.
 */
export function DescriptionField({
  id: inputId,
  repoId,
  name,
  value,
  onChange,
  disabled,
  placeholder,
  inputClassName,
}: Props) {
  const { t } = useTranslation();
  const config = useAiConfig();
  const [running, setRunning] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [previous, setPrevious] = useState<{ text: string; applied: string } | null>(null);
  const latest = useRef<string | null>(null);
  const enabled = Boolean(config?.enabled);

  const run = async () => {
    if (!enabled) {
      openAiSettings();
      return;
    }
    if (running) {
      latest.current = null;
      setRunning(null);
      await commands.aiCancel(running).catch(() => false);
      return;
    }
    const id = newRequestId("describe");
    latest.current = id;
    setRunning(id);
    setError(null);
    try {
      const text = await commands.aiDescribeRepo(
        repoId,
        id,
        useSettings.getState().language,
        name.trim(),
      );
      if (latest.current !== id) return;
      setPrevious({ text: value, applied: text });
      onChange(text);
    } catch (e) {
      if (latest.current === id && !(isAiError(e) && e.kind === "cancelled")) {
        setError(aiErrorText(t, e));
      }
    } finally {
      if (latest.current === id) {
        latest.current = null;
        setRunning(null);
      }
    }
  };

  const title = running
    ? t("ai.generatingCancel")
    : !enabled
      ? t("publish.describeSetup")
      : config && sendsCodeAway(config)
        ? `${t("publish.describe")}\n${t("publish.describeSendsTo", { service: serviceName(config) })}`
        : t("publish.describe");
  const canUndo = previous !== null && previous.applied === value;

  return (
    <div className="space-y-0.5">
      <div className="flex items-center gap-1">
        <input
          id={inputId}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
          disabled={disabled}
          className={inputClassName}
          aria-busy={Boolean(running)}
        />
        {canUndo && (
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="shrink-0"
            title={t("ai.undo")}
            aria-label={t("ai.undo")}
            onClick={() => {
              onChange(previous.text);
              setPrevious(null);
            }}
          >
            <Undo2 />
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="shrink-0 text-primary"
          disabled={disabled}
          title={title}
          aria-label={running ? t("ai.generatingCancel") : t("publish.describe")}
          aria-busy={Boolean(running)}
          onClick={() => void run()}
        >
          {running ? <LoaderCircle className="animate-spin" /> : <Sparkles />}
        </Button>
      </div>
      {running && (
        <p className="text-[11px] text-muted-foreground" role="status">
          {t("publish.describing")}
        </p>
      )}
      {error && (
        <p role="alert" className="text-[11px] text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
