import { useEffect, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  Check,
  CircleCheckBig,
  Download,
  FolderOpen,
  GitBranch,
  Info,
  KeyRound,
  Minus,
  RotateCcw,
  Scale,
  Users,
  X,
} from "lucide-react";
import { cn } from "./cn";
import { translate, type Language, type MessageKey } from "./i18n";
import * as ipc from "./ipc";
import {
  STEPS,
  folderError,
  folderErrorKey,
  stageProgress,
  type SetupInfo,
  type Stage,
  type Step,
} from "./model";
import { Button, Checkbox, Notice } from "./ui";

type T = (key: MessageKey, params?: Record<string, string>) => string;

const MIN_INSTALL_PAGE_MS = 1500;

export default function App({ initialLanguage }: { initialLanguage: Language }) {
  const [lang, setLang] = useState(initialLanguage);
  const t: T = (key, params) => translate(lang, key, params);
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [step, setStep] = useState<Step>("welcome");
  const [accepted, setAccepted] = useState(false);
  const [dir, setDir] = useState("");
  const [desktopShortcut, setDesktopShortcut] = useState(true);
  const [dirError, setDirError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [stage, setStage] = useState<Stage | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [launch, setLaunch] = useState(true);

  useEffect(() => {
    void ipc.setupInfo().then((i) => {
      setInfo(i);
      setDir(i.defaultDir);
      setRunning(i.appRunning);
    });
  }, []);

  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = t("window.title");
  });

  useEffect(() => {
    if (step === "location") void ipc.appRunning().then(setRunning);
  }, [step]);

  useEffect(() => ipc.onStage(setStage), []);

  if (!info) return <div className="h-full bg-bg" data-tauri-drag-region />;

  const busy = step === "install" && failure === null;
  const index = STEPS.indexOf(step);

  const startInstall = async () => {
    const target = dir.trim();
    try {
      await ipc.checkFolder(target);
    } catch (e) {
      setDirError(ipc.errorCode(e));
      return;
    }
    setFailure(null);
    setStage(null);
    setStep("install");
    try {
      const started = Date.now();
      await ipc.install(target, desktopShortcut);
      // An update over the same version takes a fraction of a second; don't just flash the page.
      await new Promise((r) =>
        setTimeout(r, Math.max(0, MIN_INSTALL_PAGE_MS - (Date.now() - started))),
      );
      setStep("done");
    } catch (e) {
      setFailure(ipc.errorCode(e));
    }
  };

  const finish = () => {
    if (launch) void ipc.launchAndQuit(dir.trim()).catch(() => ipc.quit());
    else void ipc.quit();
  };

  return (
    <div className="flex h-full overflow-hidden border border-black/10 bg-bg dark:border-white/10">
      <Sidebar t={t} step={step} version={info.version} />
      <main className="flex min-w-0 flex-1 flex-col">
        <TitleBar t={t} closable={!busy} />
        <div className="min-h-0 flex-1 overflow-y-auto px-10 pt-2 pb-6">
          <div key={step} className="setup-page">
            {step === "welcome" && <WelcomePage t={t} info={info} lang={lang} setLang={setLang} />}
            {step === "license" && (
              <LicensePage
                t={t}
                text={info.license}
                accepted={accepted}
                setAccepted={setAccepted}
              />
            )}
            {step === "location" && (
              <LocationPage
                t={t}
                info={info}
                dir={dir}
                setDir={(d) => {
                  setDir(d);
                  setDirError(null);
                }}
                error={dirError}
                desktopShortcut={desktopShortcut}
                setDesktopShortcut={setDesktopShortcut}
                running={running}
              />
            )}
            {step === "install" && <InstallPage t={t} stage={stage} failure={failure} />}
            {step === "done" && (
              <DonePage
                t={t}
                version={info.version}
                dir={dir}
                launch={launch}
                setLaunch={setLaunch}
              />
            )}
          </div>
        </div>
        <footer className="flex shrink-0 items-center justify-between gap-2 border-t bg-surface px-10 py-4">
          {step === "done" || busy ? (
            <span />
          ) : (
            <Button variant="ghost" onClick={() => void ipc.quit()}>
              {failure ? t("button.close") : t("button.cancel")}
            </Button>
          )}
          <div className="flex items-center gap-2">
            {(step === "license" || step === "location") && (
              <Button variant="outline" onClick={() => setStep(STEPS[index - 1] ?? "welcome")}>
                <ArrowLeft />
                {t("button.back")}
              </Button>
            )}
            {step === "welcome" && (
              <Button disabled={info.existing?.newer} onClick={() => setStep("license")}>
                {t("button.next")}
                <ArrowRight />
              </Button>
            )}
            {step === "license" && (
              <Button disabled={!accepted} onClick={() => setStep("location")}>
                {t("button.next")}
                <ArrowRight />
              </Button>
            )}
            {step === "location" && (
              <Button disabled={!dir.trim()} onClick={() => void startInstall()}>
                <Download />
                {t("button.install")}
              </Button>
            )}
            {step === "install" && failure && (
              <Button onClick={() => setStep("location")}>
                <RotateCcw />
                {t("button.retry")}
              </Button>
            )}
            {step === "done" && <Button onClick={finish}>{t("button.finish")}</Button>}
          </div>
        </footer>
      </main>
    </div>
  );
}

function Sidebar({ t, step, version }: { t: T; step: Step; version: string }) {
  const index = STEPS.indexOf(step);
  return (
    <aside
      data-tauri-drag-region
      className="relative flex w-60 shrink-0 flex-col overflow-hidden bg-[linear-gradient(160deg,#d9480f_0%,#9a3412_55%,#431407_100%)] px-6 py-7 text-white"
    >
      <div className="pointer-events-none absolute -top-16 -right-20 size-56 rounded-full bg-white/10" />
      <div className="pointer-events-none absolute -bottom-24 -left-16 size-64 rounded-full bg-black/15" />
      <div className="relative space-y-3" data-tauri-drag-region>
        <img
          src="/logo.png"
          alt=""
          width={52}
          height={52}
          className="rounded-2xl shadow-lg ring-2 ring-white/25"
        />
        <div className="leading-tight">
          <div className="text-[16px] font-bold tracking-tight">NANOMultipleGitGUI</div>
          <div className="mt-0.5 text-[11.5px] text-white/75">{t("sidebar.tagline")}</div>
        </div>
      </div>
      <ol className="relative mt-10 space-y-1" aria-label={t("steps.label")}>
        {STEPS.map((s, i) => {
          const done = i < index;
          const current = s === step;
          return (
            <li
              key={s}
              aria-current={current ? "step" : undefined}
              className={cn(
                "relative flex items-center gap-3 rounded-lg px-2.5 py-2 text-[13px] transition-colors",
                current ? "bg-white/15 font-semibold" : done ? "text-white/90" : "text-white/55",
              )}
            >
              <span
                className={cn(
                  "flex size-6 shrink-0 items-center justify-center rounded-full border text-[11px] font-semibold",
                  done && "border-white bg-white text-[#9a3412]",
                  current && "border-white bg-white/20",
                  !done && !current && "border-white/40",
                )}
              >
                {done ? <Check className="size-3.5" strokeWidth={3} /> : i + 1}
              </span>
              {t(`step.${s}`)}
            </li>
          );
        })}
      </ol>
      <div className="relative mt-auto text-[11px] text-white/60">
        {t("sidebar.version", { version })}
      </div>
    </aside>
  );
}

function TitleBar({ t, closable }: { t: T; closable: boolean }) {
  const control =
    "flex h-9 w-11 items-center justify-center text-muted transition-colors hover:text-text disabled:opacity-40 [&_svg]:size-4";
  return (
    <header data-tauri-drag-region className="flex h-9 shrink-0 items-center justify-end">
      <button
        type="button"
        className={cn(control, "hover:bg-surface-2")}
        aria-label={t("window.minimize")}
        onClick={() => void ipc.minimize()}
      >
        <Minus />
      </button>
      <button
        type="button"
        className={cn(control, "hover:bg-danger hover:text-white")}
        aria-label={t("window.close")}
        disabled={!closable}
        onClick={() => void ipc.quit()}
      >
        <X />
      </button>
    </header>
  );
}

function PageTitle({ title, intro }: { title: string; intro?: string }) {
  return (
    <div className="mb-5 space-y-1.5">
      <h1 className="text-[22px] font-bold tracking-tight">{title}</h1>
      {intro && <p className="text-[13px] text-muted">{intro}</p>}
    </div>
  );
}

function WelcomePage({
  t,
  info,
  lang,
  setLang,
}: {
  t: T;
  info: SetupInfo;
  lang: Language;
  setLang: (l: Language) => void;
}) {
  const features = [
    { icon: Users, key: "welcome.feature1" },
    { icon: KeyRound, key: "welcome.feature2" },
    { icon: Scale, key: "welcome.feature3" },
  ] as const;
  const existing = info.existing;
  return (
    <div className="space-y-5">
      <PageTitle title={t("welcome.title")} intro={t("welcome.tagline")} />
      <ul className="grid gap-2.5">
        {features.map(({ icon: Icon, key }) => (
          <li
            key={key}
            className="flex items-center gap-3 rounded-lg border bg-surface px-3.5 py-3"
          >
            <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/12 text-primary">
              <Icon className="size-4" />
            </span>
            <span className="text-[13px]">{t(key)}</span>
          </li>
        ))}
      </ul>
      {existing?.newer ? (
        <Notice tone="warning" icon={<AlertTriangle />}>
          {t("welcome.newer", { installed: existing.version ?? "?" })}
        </Notice>
      ) : existing ? (
        <Notice tone="info" icon={<Info />}>
          {t(existing.same ? "welcome.repair" : "welcome.update", {
            installed: existing.version ?? "?",
            version: info.version,
          })}
        </Notice>
      ) : (
        <p className="text-[12.5px] text-muted">
          {t("welcome.perUser", { version: info.version })}
        </p>
      )}
      {!info.hasPayload && (
        <Notice tone="warning" icon={<GitBranch />}>
          {t("welcome.noPayload")}
        </Notice>
      )}
      <div className="flex items-center gap-3">
        <span className="text-[12px] font-semibold">{t("welcome.language")}</span>
        <div className="inline-flex rounded-md border bg-surface p-0.5" role="radiogroup">
          {(["en", "vi"] as const).map((l) => (
            <button
              key={l}
              type="button"
              role="radio"
              aria-checked={lang === l}
              onClick={() => setLang(l)}
              className={cn(
                "rounded px-3 py-1 text-[12px] transition-colors",
                lang === l ? "bg-primary text-primary-fg" : "text-muted hover:text-text",
              )}
            >
              {l === "en" ? "English" : "Tiếng Việt"}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function LicensePage({
  t,
  text,
  accepted,
  setAccepted,
}: {
  t: T;
  text: string;
  accepted: boolean;
  setAccepted: (a: boolean) => void;
}) {
  return (
    <div>
      <PageTitle title={t("license.title")} intro={t("license.intro")} />
      <pre
        tabIndex={0}
        className="h-[262px] overflow-y-auto rounded-lg border bg-surface px-4 py-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-muted select-text"
      >
        {text}
      </pre>
      <div className="mt-4">
        <Checkbox checked={accepted} onChange={setAccepted}>
          {t("license.accept")}
        </Checkbox>
      </div>
    </div>
  );
}

function LocationPage({
  t,
  info,
  dir,
  setDir,
  error,
  desktopShortcut,
  setDesktopShortcut,
  running,
}: {
  t: T;
  info: SetupInfo;
  dir: string;
  setDir: (d: string) => void;
  error: string | null;
  desktopShortcut: boolean;
  setDesktopShortcut: (v: boolean) => void;
  running: boolean;
}) {
  const known = folderError(error);
  const defaultDir = info.existing?.dir ?? info.defaultDir;
  return (
    <div className="space-y-5">
      <PageTitle title={t("location.title")} intro={t("location.intro")} />
      <div className="space-y-2">
        <label htmlFor="install-dir" className="text-[12px] font-semibold">
          {t("location.folder")}
        </label>
        <div className="flex gap-2">
          <input
            id="install-dir"
            value={dir}
            spellCheck={false}
            onChange={(e) => setDir(e.target.value)}
            aria-invalid={error !== null}
            className={cn(
              "h-9 min-w-0 flex-1 rounded-md border bg-surface px-3 font-mono text-[12px] outline-none focus:border-primary",
              error && "border-danger",
            )}
          />
          <Button
            variant="outline"
            onClick={() => void ipc.pickFolder(dir).then((d) => d && setDir(d))}
          >
            <FolderOpen />
            {t("location.browse")}
          </Button>
        </div>
        {error && (
          <p className="text-[12px] text-danger">
            {known ? t(folderErrorKey(known), { example: info.defaultDir }) : error}
          </p>
        )}
        {dir.trim() !== defaultDir && (
          <button
            type="button"
            className="text-[12px] text-primary hover:underline"
            onClick={() => setDir(defaultDir)}
          >
            {t("location.reset")}
          </button>
        )}
      </div>
      <div className="space-y-2 rounded-lg border bg-surface px-4 py-3.5">
        <Checkbox checked={desktopShortcut} onChange={setDesktopShortcut}>
          {t("location.desktop")}
        </Checkbox>
        <p className="pl-6.5 text-[12px] text-muted">{t("location.startMenu")}</p>
      </div>
      {running && (
        <Notice tone="warning" icon={<AlertTriangle />}>
          {t("location.running")}
        </Notice>
      )}
      <p className="text-[12.5px] text-muted">{t("location.summary", { version: info.version })}</p>
    </div>
  );
}

/** Milliseconds since `active` turned on (or `resetKey` changed). */
function useElapsed(active: boolean, resetKey: unknown): number {
  const [tick, setTick] = useState({ key: resetKey, elapsed: 0 });
  useEffect(() => {
    if (!active) return;
    const started = Date.now();
    const id = window.setInterval(
      () => setTick({ key: resetKey, elapsed: Date.now() - started }),
      120,
    );
    return () => window.clearInterval(id);
  }, [active, resetKey]);
  return active && tick.key === resetKey ? tick.elapsed : 0;
}

function InstallPage({ t, stage, failure }: { t: T; stage: Stage | null; failure: string | null }) {
  const elapsed = useElapsed(stage === "installing" && !failure, stage);
  if (failure) {
    const known = folderError(failure);
    return (
      <div className="space-y-4">
        <PageTitle title={t("failed.title")} intro={t("failed.body")} />
        {failure === "exit:2" && (
          <Notice tone="warning" icon={<AlertTriangle />}>
            {t("failed.exit2")}
          </Notice>
        )}
        {known && (
          <Notice tone="danger" icon={<AlertTriangle />}>
            {t(folderErrorKey(known), {
              example: "C:\\Users\\…\\AppData\\Local\\NANOMultipleGitGUI",
            })}
          </Notice>
        )}
        <p className="font-mono text-[11.5px] text-muted">
          {t("failed.details", { code: failure })}
        </p>
      </div>
    );
  }
  const progress = stageProgress(stage, elapsed);
  return (
    <div>
      <PageTitle title={t("install.title")} intro={t("install.hint")} />
      <div className="mt-10 space-y-3">
        <div className="flex items-center justify-between text-[13px]">
          <span aria-live="polite">{t(`install.${stage ?? "extracting"}`)}</span>
          <span className="font-mono text-[12px] text-muted">{Math.round(progress)}%</span>
        </div>
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(progress)}
          className="relative h-2.5 overflow-hidden rounded-full bg-surface-2"
        >
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-300 ease-out"
            style={{ width: `${progress}%` }}
          />
          <div className="setup-shimmer absolute inset-y-0 left-0 w-1/3 bg-gradient-to-r from-transparent via-white/35 to-transparent" />
        </div>
      </div>
    </div>
  );
}

function DonePage({
  t,
  version,
  dir,
  launch,
  setLaunch,
}: {
  t: T;
  version: string;
  dir: string;
  launch: boolean;
  setLaunch: (v: boolean) => void;
}) {
  return (
    <div className="space-y-5">
      <div className="flex size-14 items-center justify-center rounded-full bg-success/12 text-success">
        <CircleCheckBig className="size-7" />
      </div>
      <PageTitle title={t("done.title")} intro={t("done.body", { version, dir: dir.trim() })} />
      <Notice tone="info" icon={<Info />}>
        {t("done.next")}
      </Notice>
      <Checkbox checked={launch} onChange={setLaunch}>
        {t("done.launch")}
      </Checkbox>
    </div>
  );
}
