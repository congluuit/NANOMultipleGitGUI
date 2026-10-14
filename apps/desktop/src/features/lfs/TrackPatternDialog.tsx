import { useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { isValidPattern } from "./patterns";

interface Props {
  open: boolean;
  busy?: boolean;
  onSubmit: (pattern: string, lockable: boolean) => void;
  onCancel: () => void;
}

/** "Track pattern…": a `.gitattributes` pattern and the lockable flag. */
export function TrackPatternDialog({ open, busy, onSubmit, onCancel }: Props) {
  const { t } = useTranslation();
  const [pattern, setPattern] = useState("");
  const [lockable, setLockable] = useState(false);
  const valid = isValidPattern(pattern);
  const showError = pattern.length > 0 && !valid;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="sm:max-w-md">
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (valid && !busy) onSubmit(pattern, lockable);
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("lfs.trackTitle")}</DialogTitle>
            <DialogDescription>{t("lfs.trackDesc")}</DialogDescription>
          </DialogHeader>
          <label className="grid gap-1.5 text-xs">
            <span className="text-muted-foreground">{t("lfs.pattern")}</span>
            <input
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder={t("lfs.patternPlaceholder")}
              aria-invalid={showError}
              autoFocus
              spellCheck={false}
              className="h-8 rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none aria-invalid:border-danger"
            />
            {showError && (
              <span className="text-[11px] text-danger">{t("lfs.patternInvalid")}</span>
            )}
          </label>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={lockable}
              onChange={(e) => setLockable(e.target.checked)}
              className="accent-primary"
            />
            {t("lfs.lockableLabel")}
          </label>
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!valid || busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("lfs.trackSubmit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
