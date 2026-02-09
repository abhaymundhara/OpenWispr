import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "framer-motion";
import {
  ArrowRight,
  Download,
  LayoutGrid, // Added
  BarChart3, // Added
  House,
  BookText,
  Scissors,
  Type,
  NotebookPen,
  CircleHelp,
  Bell,
  UserRound,
  SlidersHorizontal,
  Monitor,
  Keyboard,
  Languages,
  Mic,
  CheckCircle2,
  Library,
  LoaderCircle,
  Users,
  CreditCard,
  ShieldCheck,
  Hash,
  Settings2,
} from "lucide-react";
// --- Types ---

interface ModelInfo {
  name: string;
  size: number;
  downloaded: boolean;
  notes?: string;
  distro?: string;
  runtime?: string;
  sha256?: string;
  url?: string;
  can_download?: boolean;
  note?: string;
}

interface ModelDownloadProgressEvent {
  model: string;
  stage: string;
  downloaded_bytes: number;
  total_bytes?: number;
  percent?: number;
  done: boolean;
  error?: string;
  message?: string;
}

type TranscriptionStatus = "idle" | "listening" | "processing" | "error";

type TranscriptionStatusEvent = {
  status: TranscriptionStatus;
  error?: string;
};

const MODEL_SIZE_HINTS: Record<string, string> = {
  tiny: "~75 MB",
  "tiny.en": "~75 MB",
  base: "~140 MB",
  "base.en": "~140 MB",
  small: "~460 MB",
  "small.en": "~460 MB",
  medium: "~1.5 GB",
  "medium.en": "~1.5 GB",
  "large-v3-turbo": "~1.6 GB",
  "large-v3": "~3.1 GB",
  "sherpa-onnx/parakeet-tdt-0.6b-v2-int8": "~1.0 GB",
  "mlx-community/parakeet-tdt-0.6b-v2": "~1.2 GB",
  "distil-whisper-small.en": "~400 MB",
  "distil-whisper-medium.en": "~800 MB",
  "distil-whisper-large-v3": "~1.5 GB",
  "sherpa-onnx-whisper-tiny.en": "~75 MB",
  "sherpa-onnx-whisper-base.en": "~145 MB",
  "sherpa-onnx-whisper-small.en": "~480 MB",
};

// --- Components ---

const CleanButton = ({
  onClick,
  disabled,
  className,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  children: React.ReactNode;
}) => (
  <button
    onClick={onClick}
    disabled={disabled}
    className={`inline-flex min-h-11 items-center justify-center rounded-xl border border-white/15 bg-white/[0.04] px-4 text-sm font-medium text-white/90 transition-all duration-200 hover:border-white/25 hover:bg-white/[0.08] active:scale-[0.98] ${className || ""} ${disabled ? "pointer-events-none opacity-45" : ""}`}
  >
    {children}
  </button>
);

type SettingsSection =
  | "general"
  | "system"
  | "models"
  | "account"
  | "team"
  | "billing"
  | "privacy";

type AppNavItemProps = {
  active?: boolean;
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
};

const AppNavItem = ({ active, icon, label, onClick }: AppNavItemProps) => (
  <button
    onClick={onClick}
    className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-[1.02rem] transition-colors ${
      active
        ? "bg-white/[0.12] text-white"
        : "text-white/78 hover:bg-white/[0.06]"
    }`}
  >
    <span className={`${active ? "text-white/95" : "text-white/65"}`}>
      {icon}
    </span>
    <span>{label}</span>
  </button>
);

type SettingsNavItemProps = {
  active?: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
};

const SettingsNavItem = ({
  active,
  icon,
  label,
  onClick,
}: SettingsNavItemProps) => (
  <button
    onClick={onClick}
    className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-[1.02rem] transition-colors ${
      active
        ? "bg-white/[0.10] text-white"
        : "text-white/72 hover:bg-white/[0.06]"
    }`}
  >
    <span className={`${active ? "text-white/92" : "text-white/62"}`}>
      {icon}
    </span>
    <span>{label}</span>
  </button>
);

type SettingsRowProps = {
  title: string;
  description: string;
  actionLabel: React.ReactNode;
  onAction?: () => void;
  actionDisabled?: boolean;
  compact?: boolean;
};

const SettingsRow = ({
  title,
  description,
  actionLabel,
  onAction,
  actionDisabled,
  compact,
}: SettingsRowProps) => (
  <div
    className={`grid gap-3 ${compact ? "py-3" : "py-4"} border-b border-white/10 last:border-b-0 sm:grid-cols-[1fr_auto] sm:items-center`}
  >
    <div>
      <p className="text-[1.02rem] font-semibold text-white/95">{title}</p>
      <p className="mt-1 text-[0.95rem] text-white/64">{description}</p>
    </div>
    <CleanButton
      onClick={onAction ?? (() => {})}
      disabled={actionDisabled}
      className="h-[2.8rem] min-w-[10.5rem] rounded-[0.85rem] bg-white/[0.07] text-[1.02rem] sm:justify-center"
    >
      {actionLabel}
    </CleanButton>
  </div>
);

function Dashboard() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [activeDownload, setActiveDownload] = useState<string>();
  const [downloadProgress, setDownloadProgress] = useState<
    Record<string, ModelDownloadProgressEvent>
  >({});
  const [activeModel, setActiveModel] = useState<string>();
  const [section, setSection] = useState<SettingsSection>("general");
  const [settingsOpen, setSettingsOpen] = useState(false);

  const loadModels = async () => {
    setLoading(true);
    setError(undefined);
    try {
      const [data, selected] = await Promise.all([
        invoke<ModelInfo[]>("list_models"),
        invoke<string>("get_active_model"),
      ]);
      setModels(data);
      setActiveModel(selected);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadModels();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const setup = async () => {
      unlisten = await listen<ModelDownloadProgressEvent>(
        "model-download-progress",
        (event) => {
          const progress = event.payload;
          setDownloadProgress((prev) => ({
            ...prev,
            [progress.model]: progress,
          }));
          if (progress.error) {
            setError(progress.error);
          }
        },
      );
    };

    void setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const onDownload = async (model: string) => {
    setActiveDownload(model);
    setError(undefined);
    setDownloadProgress((prev) => ({
      ...prev,
      [model]: {
        model,
        stage: "queued",
        downloaded_bytes: 0,
        total_bytes: undefined,
        percent: 0,
        done: false,
        error: undefined,
        message: "Queued",
      },
    }));

    try {
      await invoke("download_model", { model });
      await loadModels();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setActiveDownload(undefined);
    }
  };

  const onSelectModel = async (model: string) => {
    setError(undefined);
    try {
      await invoke("set_active_model", { model });
      setActiveModel(model);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const downloadedModels = models.filter((m) => m.downloaded);
  const activeModelInfo = models.find((m) => m.name === activeModel);
  const libraryModels = models.filter((m) => m.can_download);
  const installableModels = models.filter(
    (m) => m.can_download && !m.downloaded,
  );
  const selectedSectionTitle =
    section === "general"
      ? "General"
      : section === "system"
        ? "System"
        : section === "models"
          ? "Model Library"
          : section === "account"
            ? "Account"
            : section === "team"
              ? "Team"
              : section === "billing"
                ? "Plans and Billing"
                : "Data and Privacy";

  const sectionSummary =
    section === "general"
      ? "Core dictation controls and defaults."
      : section === "system"
        ? "Desktop behavior and app-level options."
        : section === "models"
          ? "Download, activate, and manage speech models."
          : "Coming soon.";

  return (
    <div
      className="relative h-screen overflow-hidden bg-[#0b0c10] text-white"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_16%_8%,rgba(255,255,255,0.10)_0%,transparent_38%),radial-gradient(circle_at_92%_6%,rgba(143,178,218,0.12)_0%,transparent_30%),linear-gradient(160deg,#0a0b0e_0%,#111317_45%,#0a0b0d_100%)]" />

      {/* Global Drag Region */}
      <div
        className="absolute left-0 right-0 top-0 h-10 z-10"
        style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
      />

      <div className="relative flex h-full">
        <aside
          className="hidden w-[14.5rem] border-r border-white/10 bg-black/20 px-2 pb-3 pt-10 backdrop-blur-xl sm:flex sm:flex-col"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <div className="mb-6 px-2">
            <div className="flex items-center gap-2">
              <div className="grid h-6 w-6 place-items-center rounded-md bg-white/[0.08]">
                <Library className="h-3.5 w-3.5 text-white/90" />
              </div>
              <p className="text-[2rem] font-semibold leading-none tracking-tight">
                OpenWispr
              </p>
              <span className="rounded-md border border-white/25 px-2 py-0.5 text-[0.9rem] text-white/78">
                Local
              </span>
            </div>
          </div>

          <div className="space-y-1">
            <AppNavItem
              active={!settingsOpen}
              icon={<LayoutGrid className="h-[1.05rem] w-[1.05rem]" />}
              label="Dashboard"
              onClick={() => setSettingsOpen(false)}
            />
            <AppNavItem
              icon={<BarChart3 className="h-[1.05rem] w-[1.05rem]" />}
              label="Analytics"
            />
            <AppNavItem
              icon={<BookText className="h-[1.05rem] w-[1.05rem]" />}
              label="Dictionary"
            />
          </div>

          <div className="mt-auto">
            <div className="space-y-1 border-t border-white/10 pt-3">
              <AppNavItem
                active={settingsOpen}
                icon={<Settings2 className="h-[1.05rem] w-[1.05rem]" />}
                label="Settings"
                onClick={() => {
                  setSection("general");
                  setSettingsOpen(true);
                }}
              />
              <AppNavItem
                icon={<CircleHelp className="h-[1.05rem] w-[1.05rem]" />}
                label="Help"
              />
            </div>
          </div>
        </aside>

        <main
          className="relative flex flex-1 overflow-hidden p-3 sm:p-5"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <div className="flex h-full w-full flex-col rounded-[1.2rem] border border-white/10 bg-white/[0.03] p-8 backdrop-blur-lg">
            <div className="mb-0 flex items-center justify-between">
              <div>
                <h1 className="text-xl font-medium text-white/96">
                  Good evening, Abhay
                </h1>
                <p className="mt-1 text-sm text-white/58">
                  Press{" "}
                  <kbd className="rounded bg-white/10 px-1.5 py-0.5 font-sans text-xs font-medium text-white/80">
                    Fn
                  </kbd>{" "}
                  in any text box to start dictating
                </p>
              </div>
              <div className="flex gap-2">
                <button className="grid h-8 w-8 place-items-center rounded-full border border-white/10 bg-white/5 text-white/65 transition-colors hover:bg-white/10 hover:text-white/90">
                  <CircleHelp className="h-4 w-4" />
                </button>
                <button className="grid h-8 w-8 place-items-center rounded-full border border-white/10 bg-white/5 text-white/65 transition-colors hover:bg-white/10 hover:text-white/90">
                  <ArrowRight className="h-4 w-4" />
                </button>
              </div>
            </div>

            <div className="mt-8">
              <div className="relative overflow-hidden rounded-2xl border border-white/10 bg-[#16181d] px-8 py-10 shadow-lg">
                <div className="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 opacity-20">
                  <div className="h-32 w-32 rounded-full bg-blue-500 blur-3xl"></div>
                </div>
                <div className="relative z-10 flex items-center justify-between">
                  <div>
                    <div className="flex items-baseline gap-3">
                      <span className="text-5xl font-semibold tracking-tight text-white/96">
                        11 mins
                      </span>
                      <span className="text-lg text-white/58">
                        lifetime saved
                      </span>
                    </div>
                    <div className="mt-2 text-sm text-white/40">
                      38 sessions • all time
                    </div>
                  </div>
                  <div className="flex h-2 w-2 rounded-full bg-white/20"></div>
                </div>
              </div>
            </div>

            <div className="mt-8 grid grid-cols-3 gap-12 px-2">
              <div className="text-center">
                <div className="text-4xl font-semibold tracking-tight text-white/96">
                  1
                </div>
                <div className="mt-1 text-sm font-medium text-white/58">
                  Day Streak
                </div>
              </div>
              <div className="text-center">
                <div className="text-4xl font-semibold tracking-tight text-white/96">
                  165
                </div>
                <div className="mt-1 text-sm font-medium text-white/58">
                  Avg WPM
                </div>
              </div>
              <div className="text-center">
                <div className="text-4xl font-semibold tracking-tight text-white/96">
                  384
                </div>
                <div className="mt-1 text-sm font-medium text-white/58">
                  Words
                </div>
              </div>
            </div>

            <div className="mt-12">
              <h3 className="mb-4 text-base font-medium text-white/90">
                Quick Actions
              </h3>
              <div className="grid grid-cols-2 gap-4">
                <button className="group relative flex flex-col justify-between rounded-xl border border-white/10 bg-white/[0.03] p-5 text-left transition-all hover:bg-white/[0.06] hover:border-white/15 active:scale-[0.99]">
                  <div>
                    <h4 className="text-base font-medium text-white/90 group-hover:text-white">
                      View Analytics
                    </h4>
                    <p className="mt-1 text-sm text-white/50 group-hover:text-white/60">
                      Detailed insights
                    </p>
                  </div>
                </button>
                <button className="group relative flex flex-col justify-between rounded-xl border border-white/10 bg-white/[0.03] p-5 text-left transition-all hover:bg-white/[0.06] hover:border-white/15 active:scale-[0.99]">
                  <div>
                    <h4 className="text-base font-medium text-white/90 group-hover:text-white">
                      Custom Dictionary
                    </h4>
                    <p className="mt-1 text-sm text-white/50 group-hover:text-white/60">
                      Manage words
                    </p>
                  </div>
                </button>
              </div>
            </div>
          </div>

          {settingsOpen && (
            <div
              className="absolute inset-0 z-40 flex items-center justify-center bg-black/45 p-3 sm:p-5"
              onClick={() => setSettingsOpen(false)}
            >
              <div
                className="h-[min(90vh,760px)] w-full max-w-[1110px] rounded-[1.35rem] border border-white/10 bg-white/[0.03] p-3 shadow-[0_30px_70px_rgba(0,0,0,0.45)] backdrop-blur-xl sm:p-4"
                onClick={(e) => e.stopPropagation()}
              >
                <div className="grid h-full overflow-hidden rounded-[1.15rem] border border-white/10 bg-[#13151a]/95 sm:grid-cols-[16.8rem_1fr]">
                  <section className="border-b border-white/10 bg-white/[0.02] px-3 py-3 sm:border-b-0 sm:border-r sm:px-4 sm:py-5">
                    <p className="mb-3 text-[0.84rem] font-semibold uppercase tracking-[0.16em] text-white/48">
                      Settings
                    </p>

                    <div className="space-y-1">
                      <SettingsNavItem
                        active={section === "general"}
                        icon={
                          <SlidersHorizontal className="h-[1.05rem] w-[1.05rem]" />
                        }
                        label="General"
                        onClick={() => setSection("general")}
                      />
                      <SettingsNavItem
                        active={section === "system"}
                        icon={<Monitor className="h-[1.05rem] w-[1.05rem]" />}
                        label="System"
                        onClick={() => setSection("system")}
                      />
                      <SettingsNavItem
                        active={section === "models"}
                        icon={<Hash className="h-[1.05rem] w-[1.05rem]" />}
                        label="Models"
                        onClick={() => setSection("models")}
                      />
                    </div>

                    <div className="my-4 h-px bg-white/10" />

                    <p className="mb-2 text-[0.84rem] font-semibold uppercase tracking-[0.16em] text-white/48">
                      Account
                    </p>
                    <div className="space-y-1">
                      <SettingsNavItem
                        active={section === "account"}
                        icon={<UserRound className="h-[1.05rem] w-[1.05rem]" />}
                        label="Account"
                        onClick={() => setSection("account")}
                      />
                      <SettingsNavItem
                        active={section === "team"}
                        icon={<Users className="h-[1.05rem] w-[1.05rem]" />}
                        label="Team"
                        onClick={() => setSection("team")}
                      />
                      <SettingsNavItem
                        active={section === "billing"}
                        icon={
                          <CreditCard className="h-[1.05rem] w-[1.05rem]" />
                        }
                        label="Plans and Billing"
                        onClick={() => setSection("billing")}
                      />
                      <SettingsNavItem
                        active={section === "privacy"}
                        icon={
                          <ShieldCheck className="h-[1.05rem] w-[1.05rem]" />
                        }
                        label="Data and Privacy"
                        onClick={() => setSection("privacy")}
                      />
                    </div>
                  </section>

                  <section className="min-h-0 overflow-y-auto px-3 py-4 sm:px-6 sm:py-6">
                    <div className="mb-4 sm:mb-5 flex items-start justify-between gap-3">
                      <div>
                        <h2 className="text-[2rem] font-semibold tracking-tight text-white/96">
                          {selectedSectionTitle}
                        </h2>
                        <p className="mt-1 text-[1.02rem] text-white/62">
                          {sectionSummary}
                        </p>
                      </div>
                      <CleanButton
                        onClick={() => setSettingsOpen(false)}
                        className="min-w-[7rem]"
                      >
                        Close
                      </CleanButton>
                    </div>

                    {error && (
                      <motion.div
                        initial={{ opacity: 0, y: -6 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="mb-4 rounded-xl border border-[#7C3436] bg-[#3A1618]/70 px-4 py-3 text-sm text-[#F7D9D9]"
                      >
                        {error}
                      </motion.div>
                    )}

                    {loading && models.length === 0 ? (
                      <div className="space-y-2">
                        {[...Array(4)].map((_, idx) => (
                          <div
                            key={`skeleton-${idx}`}
                            className="h-[4.75rem] animate-pulse rounded-xl border border-white/10 bg-white/[0.03]"
                          />
                        ))}
                      </div>
                    ) : (
                      <>
                        {(section === "general" || section === "system") && (
                          <div className="rounded-2xl border border-white/10 bg-white/[0.04] px-4 sm:px-5">
                            <SettingsRow
                              title="Keyboard shortcuts"
                              description="Hold Fn and speak. Learn more soon."
                              actionLabel="Change"
                            />
                            <SettingsRow
                              title="Microphone"
                              description="Use your system default input device."
                              actionLabel="Change"
                            />
                            <SettingsRow
                              title="Languages"
                              description="English (auto detect planned)."
                              actionLabel="Change"
                            />
                            <SettingsRow
                              title="Active model"
                              description={
                                activeModelInfo
                                  ? `${activeModelInfo.name} · ${activeModelInfo.runtime ?? "whisper.cpp"}`
                                  : "No model selected"
                              }
                              actionLabel="Choose"
                              onAction={() => setSection("models")}
                            />
                          </div>
                        )}

                        {section === "models" && (
                          <div className="rounded-2xl border border-white/10 bg-white/[0.04] px-4 sm:px-5">
                            <SettingsRow
                              compact
                              title="Installed models"
                              description={`${downloadedModels.length} installed`}
                              actionLabel={
                                <span className="inline-flex items-center gap-2">
                                  Refresh
                                  <ArrowRight className="h-4 w-4" />
                                </span>
                              }
                              onAction={() => {
                                void loadModels();
                              }}
                              actionDisabled={loading}
                            />

                            {libraryModels.map((model) => {
                              const isActive = activeModel === model.name;
                              const isDownloading =
                                activeDownload === model.name;
                              const busy = !!activeDownload && !isDownloading;
                              const percent =
                                typeof downloadProgress[model.name]?.percent ===
                                "number"
                                  ? Math.round(
                                      downloadProgress[model.name].percent ?? 0,
                                    )
                                  : 0;

                              return (
                                <SettingsRow
                                  key={model.name}
                                  compact
                                  title={model.name}
                                  description={`${MODEL_SIZE_HINTS[model.name] || "Unknown size"} · ${model.runtime ?? "whisper.cpp"}`}
                                  actionLabel={
                                    isActive ? (
                                      "Active"
                                    ) : isDownloading ? (
                                      <span className="inline-flex items-center gap-2">
                                        <LoaderCircle className="h-4 w-4 animate-spin" />
                                        {percent}%
                                      </span>
                                    ) : model.downloaded ? (
                                      "Activate"
                                    ) : (
                                      "Download"
                                    )
                                  }
                                  onAction={() => {
                                    if (isActive || busy) return;
                                    if (model.downloaded) {
                                      void onSelectModel(model.name);
                                    } else {
                                      void onDownload(model.name);
                                    }
                                  }}
                                  actionDisabled={isActive || busy}
                                />
                              );
                            })}
                          </div>
                        )}

                        {(section === "account" ||
                          section === "team" ||
                          section === "billing" ||
                          section === "privacy") && (
                          <div className="rounded-2xl border border-white/10 bg-white/[0.04] px-5 py-8 text-center">
                            <p className="text-[1.1rem] font-medium text-white/92">
                              {selectedSectionTitle}
                            </p>
                            <p className="mt-2 text-[0.98rem] text-white/58">
                              This section is planned. Core dictation and model
                              controls are live now.
                            </p>
                          </div>
                        )}
                      </>
                    )}
                  </section>
                </div>
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

// --- Dictation Pill Components ---

const useFeedbackSounds = (enabled: boolean) => {
  const ctxRef = useRef<AudioContext | null>(null);
  const lastPlayedAtRef = useRef(0);

  const getContext = () => {
    if (ctxRef.current) return ctxRef.current;
    const Ctx =
      (window.AudioContext as typeof AudioContext | undefined) ||
      ((window as Window & { webkitAudioContext?: typeof AudioContext })
        .webkitAudioContext as typeof AudioContext | undefined);
    if (!Ctx) return null;
    ctxRef.current = new Ctx();
    return ctxRef.current;
  };

  const playStartSound = () => {
    if (!enabled) return;
    const now = Date.now();
    if (now - lastPlayedAtRef.current < 40) return;
    lastPlayedAtRef.current = now;

    const ctx = getContext();
    if (!ctx) return;
    if (ctx.state === "suspended") void ctx.resume();
    const t = ctx.currentTime;

    const playTone = (
      freq: number,
      gainStart: number,
      startOffset: number,
      duration: number,
    ) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.frequency.setValueAtTime(freq, t + startOffset);
      gain.gain.setValueAtTime(gainStart, t + startOffset);
      gain.gain.exponentialRampToValueAtTime(
        0.0001,
        t + startOffset + duration,
      );
      osc.start(t + startOffset);
      osc.stop(t + startOffset + duration);
    };

    playTone(350, 0.01, 0.0, 0.08);
    playTone(500, 0.008, 0.05, 0.08);
    playTone(750, 0.006, 0.1, 0.1);
  };

  const playStopSound = () => {
    if (!enabled) return;
    const now = Date.now();
    if (now - lastPlayedAtRef.current < 40) return;
    lastPlayedAtRef.current = now;

    const ctx = getContext();
    if (!ctx) return;
    if (ctx.state === "suspended") void ctx.resume();
    const t = ctx.currentTime;

    const osc1 = ctx.createOscillator();
    const gain1 = ctx.createGain();
    osc1.connect(gain1);
    gain1.connect(ctx.destination);
    osc1.frequency.setValueAtTime(600, t);
    osc1.frequency.exponentialRampToValueAtTime(400, t + 0.1);
    gain1.gain.setValueAtTime(0.008, t);
    gain1.gain.exponentialRampToValueAtTime(0.0001, t + 0.15);
    osc1.start(t);
    osc1.stop(t + 0.15);

    const osc2 = ctx.createOscillator();
    const gain2 = ctx.createGain();
    osc2.connect(gain2);
    gain2.connect(ctx.destination);
    osc2.frequency.setValueAtTime(350, t + 0.08);
    osc2.frequency.exponentialRampToValueAtTime(250, t + 0.2);
    gain2.gain.setValueAtTime(0.006, t + 0.08);
    gain2.gain.exponentialRampToValueAtTime(0.0001, t + 0.25);
    osc2.start(t + 0.08);
    osc2.stop(t + 0.25);
  };

  return { playStartSound, playStopSound };
};

const JarvisWaveBars = ({ audioLevel }: { audioLevel: number }) => {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => setTick((v) => v + 1), 80);
    return () => window.clearInterval(timer);
  }, []);

  const time = tick * 0.32;
  const normalizedLevel = Math.min(audioLevel / 15, 1);

  return (
    <div className="flex h-[18px] w-full items-center justify-center gap-[3px]">
      {[...Array(10)].map((_, index) => {
        const baseHeight = 6;
        const maxHeight = 14;
        const variation =
          Math.sin(time + index * 0.4) * 0.6 + (Math.random() - 0.5) * 1.0;
        const height = Math.max(
          3,
          Math.min(
            maxHeight,
            baseHeight + normalizedLevel * (maxHeight - baseHeight) + variation,
          ),
        );
        return (
          <div
            key={index}
            className="w-[3px] rounded-[1.5px] bg-white/80 transition-[height] duration-100 ease-out"
            style={{ height: `${height}px` }}
          />
        );
      })}
    </div>
  );
};

const FloatingPill = ({
  shouldRecord,
  status,
  error,
  onStop,
}: {
  shouldRecord: boolean;
  status: TranscriptionStatus;
  error?: string;
  onStop: () => void;
}) => {
  const [audioLevel, setAudioLevel] = useState(0);

  useEffect(() => {
    if (!shouldRecord) {
      setAudioLevel(0);
      return;
    }

    const unlisten = listen<number>("audio-level", (event) => {
      setAudioLevel(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
      setAudioLevel(0);
    };
  }, [shouldRecord]);

  return (
    <motion.div
      initial={{ scale: 0.8, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      exit={{ scale: 0.96, opacity: 0 }}
      transition={{ type: "spring", damping: 26, stiffness: 340, mass: 0.9 }}
      className="fixed bottom-2 left-1/2 z-[999999] -translate-x-1/2"
      onClick={() => {
        if (shouldRecord) onStop();
      }}
    >
      {status === "processing" ? (
        <div className="flex h-8 w-20 items-center justify-center rounded-2xl border border-white/20 bg-[rgba(20,20,20,0.95)] px-[15px] shadow-[0_4px_12px_rgba(0,0,0,0.2)] backdrop-blur-[15px]">
          <div className="flex gap-1.5">
            <span className="loading-dot" />
            <span className="loading-dot" />
            <span className="loading-dot" />
          </div>
        </div>
      ) : status === "error" ? (
        <div className="flex h-8 min-w-[140px] items-center justify-center rounded-2xl border border-red-300/30 bg-red-500/95 px-[15px] text-white shadow-[0_4px_12px_rgba(255,59,48,0.3)]">
          <span className="mr-1.5 text-sm">⚠️</span>
          <span className="max-w-[220px] truncate text-xs font-medium">
            {error || "Transcription error"}
          </span>
        </div>
      ) : (
        <div className="flex h-8 w-[120px] items-center rounded-2xl border border-white/20 bg-[rgba(20,20,20,0.95)] px-[15px] shadow-[0_4px_12px_rgba(0,0,0,0.2)] backdrop-blur-[15px]">
          <JarvisWaveBars audioLevel={audioLevel} />
        </div>
      )}
    </motion.div>
  );
};

function DictationPillApp() {
  const [fnHeld, setFnHeld] = useState(false);
  const [sttStatus, setSttStatus] = useState<TranscriptionStatus>("idle");
  const [sttError, setSttError] = useState<string>();
  const previousFnHeld = useRef(false);
  const fnHeldRef = useRef(false);
  const { playStartSound, playStopSound } = useFeedbackSounds(true);

  // Keep ref in sync with state
  useEffect(() => {
    fnHeldRef.current = fnHeld;
  }, [fnHeld]);

  useEffect(() => {
    let unlistenHold: (() => void) | undefined;
    let unlistenToggle: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    const setupListener = async () => {
      try {
        unlistenHold = await listen<boolean>("fn-hold", (event) => {
          setFnHeld(event.payload);
        });
        unlistenToggle = await listen("global-shortcut-pressed", () => {
          setFnHeld((prev) => !prev);
        });
        unlistenStatus = await listen<TranscriptionStatusEvent>(
          "transcription-status",
          (event) => {
            setSttStatus(event.payload.status);
            if (
              event.payload.status === "listening" ||
              event.payload.status === "idle"
            ) {
              setSttError(undefined);
            }
            if (event.payload.error) {
              setSttError(event.payload.error);
            } else if (event.payload.status !== "error") {
              setSttError(undefined);
            }
          },
        );
      } catch (e) {
        console.error("Tauri event listener failed", e);
      }
    };
    setupListener();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "`") {
        setFnHeld((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      if (unlistenHold) unlistenHold();
      if (unlistenToggle) unlistenToggle();
      if (unlistenStatus) unlistenStatus();
    };
  }, []);

  useEffect(() => {
    if (fnHeld && !previousFnHeld.current) {
      playStartSound();
    } else if (!fnHeld && previousFnHeld.current) {
      playStopSound();
    }
    previousFnHeld.current = fnHeld;
  }, [fnHeld, playStartSound, playStopSound]);

  useEffect(() => {
    const root = document.getElementById("root");
    document.documentElement.style.backgroundColor = "transparent";
    document.body.style.backgroundColor = "transparent";
    if (root) root.style.backgroundColor = "transparent";

    return () => {
      document.documentElement.style.backgroundColor = "#090f13";
      document.body.style.backgroundColor = "#090f13";
      if (root) root.style.backgroundColor = "#090f13";
    };
  }, []);

  const showPill = fnHeld || sttStatus !== "idle";

  return (
    <div
      className="h-screen w-screen flex items-center justify-center overflow-visible bg-transparent"
      style={{ background: "transparent", backgroundColor: "transparent" }}
    >
      <style>{`
        html, body, #root { background: transparent !important; }
        .loading-dot {
          width: 6px;
          height: 6px;
          border-radius: 9999px;
          background: rgba(255, 255, 255, 0.85);
          animation: loadingPulse 1.4s infinite ease-in-out;
        }
        .loading-dot:nth-child(1) { animation-delay: -0.32s; }
        .loading-dot:nth-child(2) { animation-delay: -0.16s; }
        .loading-dot:nth-child(3) { animation-delay: 0s; }
        @keyframes loadingPulse {
          0%, 80%, 100% {
            transform: scale(0.6);
            opacity: 0.4;
          }
          40% {
            transform: scale(1);
            opacity: 1;
            transform: scale(1);
            opacity: 1;
          }
        }
      `}</style>
      <AnimatePresence>
        {showPill && (
          <FloatingPill
            shouldRecord={fnHeld}
            status={sttStatus}
            error={sttError}
            onStop={() => {
              playStopSound();
              setFnHeld(false);
              invoke("stop_recording").catch(console.error);
            }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}

function App() {
  const [windowLabel, setWindowLabel] = useState<string>("");

  useEffect(() => {
    setWindowLabel(appWindow.label);
  }, []);

  if (!windowLabel) {
    return null;
  }

  if (windowLabel === "models") {
    return <Dashboard />;
  }

  return <DictationPillApp />;
}

export default App;
