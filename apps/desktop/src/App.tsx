import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "framer-motion";
import {
  ArrowRight,
  CheckCircle2,
  Cpu,
  Download,
  Library,
  LoaderCircle,
  MicVocal,
  Settings2,
  Sparkles,
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
    className={`inline-flex min-h-11 items-center justify-center rounded-xl border border-[#2D4952] bg-[#13242B] px-4 text-sm font-medium text-[#EDE7DD] transition-all duration-200 hover:border-[#3E6470] hover:bg-[#1A2F38] active:scale-[0.98] ${className || ""} ${disabled ? "pointer-events-none opacity-45" : ""}`}
  >
    {children}
  </button>
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
  const [currentView, setCurrentView] = useState<
    "dashboard" | "library" | "settings"
  >("dashboard");

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
    loadModels();
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
  const libraryModels = models.filter((m) => !m.downloaded && m.can_download);
  const activeDownloadProgress = activeDownload
    ? downloadProgress[activeDownload]
    : undefined;

  const stats = [
    {
      label: "Installed Models",
      value: downloadedModels.length.toString(),
      detail: downloadedModels.length > 0 ? "Ready for use" : "None installed",
      icon: <CheckCircle2 className="h-4 w-4" />,
    },
    {
      label: "Library Available",
      value: libraryModels.length.toString(),
      detail: "Download-ready",
      icon: <Library className="h-4 w-4" />,
    },
    {
      label: "Active Runtime",
      value: activeModelInfo?.runtime ?? "Not selected",
      detail: activeModelInfo ? activeModelInfo.name : "Select a model",
      icon: <Cpu className="h-4 w-4" />,
    },
  ];

  // --- Layout Helper Components ---

  const NavItem = ({
    active,
    onClick,
    icon,
    label,
  }: {
    active: boolean;
    onClick: () => void;
    icon: React.ReactNode;
    label: string;
  }) => (
    <button
      onClick={onClick}
      className={`group flex w-full items-center gap-3 rounded-xl border px-3.5 py-2.5 text-left text-sm transition-all duration-200 ${
        active
          ? "border-[#3A5F69] bg-[#1A2B32]/90 text-[#F2EEE6]"
          : "border-transparent bg-transparent text-[#BAC3C4] hover:border-[#2B434A] hover:bg-[#152027]/70 hover:text-[#F2EEE6]"
      }`}
    >
      <span
        className={`transition-transform duration-200 group-hover:scale-105 ${
          active ? "text-[#D4EEE8]" : "text-[#8DA6A9]"
        }`}
      >
        {icon}
      </span>
      <span>{label}</span>
    </button>
  );

  return (
    <div
      className="relative h-screen overflow-hidden bg-[#090F13] text-[#EFE9DF]"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_12%_10%,rgba(33,113,102,0.24)_0%,transparent_40%),radial-gradient(circle_at_92%_4%,rgba(183,143,80,0.18)_0%,transparent_35%),linear-gradient(150deg,#0a1115_0%,#111a20_45%,#090d11_100%)]" />
      <div className="relative flex h-full flex-col sm:flex-row">
        <aside
          className="w-full shrink-0 border-b border-[#2A353B]/80 bg-[linear-gradient(180deg,rgba(12,18,23,0.95)_0%,rgba(11,17,21,0.92)_100%)] sm:h-full sm:w-[16.5rem] sm:border-b-0 sm:border-r sm:border-[#2A353B]/80"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <div className="flex h-full flex-col p-4 sm:p-5">
            <div className="mb-5 flex items-center gap-3 rounded-2xl border border-[#2F4047] bg-[#121B21]/70 px-3 py-3">
              <div className="grid h-11 w-11 place-items-center rounded-xl bg-gradient-to-br from-[#1B6A64] to-[#234652] shadow-[0_10px_28px_rgba(9,18,22,0.55)]">
                <MicVocal className="h-5 w-5 text-[#EAF6F2]" />
              </div>
              <div>
                <p className="ow-display text-[1.05rem] leading-none text-[#F4EFE5]">
                  OpenWispr
                </p>
                <p className="mt-1 text-[0.7rem] uppercase tracking-[0.2em] text-[#8EA1A4]">
                  Desktop Voice
                </p>
              </div>
            </div>

            <nav className="space-y-2">
              <p className="px-2 text-[0.68rem] uppercase tracking-[0.18em] text-[#7D9195]">
                Workspace
              </p>
              <NavItem
                active={currentView === "dashboard"}
                onClick={() => setCurrentView("dashboard")}
                label="Dashboard"
                icon={<Sparkles className="h-[1.05rem] w-[1.05rem]" />}
              />
              <NavItem
                active={currentView === "library"}
                onClick={() => setCurrentView("library")}
                label="Model Library"
                icon={<Library className="h-[1.05rem] w-[1.05rem]" />}
              />
              <NavItem
                active={currentView === "settings"}
                onClick={() => setCurrentView("settings")}
                label="Settings"
                icon={<Settings2 className="h-[1.05rem] w-[1.05rem]" />}
              />
            </nav>

            <div className="mt-4 rounded-2xl border border-[#32444C] bg-[#121B21]/80 p-3.5">
              <div className="mb-2 flex items-center justify-between text-[0.67rem] uppercase tracking-[0.16em] text-[#8FA3A6]">
                <span>Current Engine</span>
                <span className="h-1.5 w-1.5 rounded-full bg-[#65D6B4]" />
              </div>
              <p className="truncate text-sm font-medium text-[#F5EFE5]">
                {activeModelInfo?.name ?? "No model selected"}
              </p>
              <p className="mt-1 text-[0.75rem] text-[#9DB0B3]">
                Runtime: {activeModelInfo?.runtime ?? "whisper.cpp"}
              </p>
            </div>

            <div className="mt-auto hidden pt-4 text-[0.75rem] leading-relaxed text-[#91A1A5] sm:block">
              Hold <span className="rounded bg-[#1A2A31] px-1.5 py-0.5 text-[#DCE8E3]">Fn</span>{" "}
              to dictate in any app.
            </div>
          </div>
        </aside>

        <main
          className="relative flex-1 overflow-hidden"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <div className="h-full overflow-y-auto px-4 py-4 sm:px-5 sm:py-5">
            <div className="mx-auto w-full max-w-5xl">
            <div className="mb-5 flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
              <div>
                <p className="mb-2 text-[0.66rem] uppercase tracking-[0.2em] text-[#8EA4A7]">
                  Voice Control Center
                </p>
                <h1 className="ow-display text-[1.7rem] leading-none text-[#F6F1E8] sm:text-[2rem]">
                  {currentView === "dashboard" && "Model Dashboard"}
                  {currentView === "library" && "Model Library"}
                  {currentView === "settings" && "Preferences"}
                </h1>
                <p className="mt-2 max-w-2xl text-sm text-[#9BAEB1]">
                  {currentView === "dashboard" &&
                    "Monitor installed engines, switch active models, and keep dictation performance ready."}
                  {currentView === "library" &&
                    "Download local speech models and activate the best runtime for your machine."}
                  {currentView === "settings" &&
                    "Personal controls for your dictation workflow are coming next."}
                </p>
              </div>
              <CleanButton
                onClick={() => {
                  void loadModels();
                }}
                disabled={loading}
                className="w-full sm:w-auto"
              >
                {loading ? (
                  <span className="inline-flex items-center gap-2">
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                    Refreshing
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-2">
                    Refresh Data
                    <ArrowRight className="h-4 w-4" />
                  </span>
                )}
              </CleanButton>
            </div>

            {error && (
              <motion.div
                initial={{ opacity: 0, y: -8 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-4 rounded-xl border border-[#7C3436] bg-[#3A1618]/70 px-4 py-3 text-sm text-[#F7D9D9]"
              >
                {error}
              </motion.div>
            )}

            {loading && models.length === 0 ? (
              <div className="space-y-3">
                {[...Array(3)].map((_, index) => (
                  <div
                    key={`loading-${index}`}
                    className="h-[84px] animate-pulse rounded-2xl border border-[#2A353B] bg-[#121B21]/70"
                  />
                ))}
              </div>
            ) : (
              <div className="space-y-4">
                {currentView === "dashboard" && (
                  <>
                    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                      {stats.map((stat) => (
                        <div
                          key={stat.label}
                          className="rounded-2xl border border-[#2F4047] bg-[#121B21]/78 p-4 shadow-[0_8px_26px_rgba(7,13,16,0.35)]"
                        >
                          <div className="mb-2 flex items-center justify-between text-[#98A9AD]">
                            <span className="text-[0.7rem] uppercase tracking-[0.14em]">
                              {stat.label}
                            </span>
                            <span className="text-[#8ECFC0]">{stat.icon}</span>
                          </div>
                          <p className="truncate text-lg font-semibold text-[#F7F2E8]">
                            {stat.value}
                          </p>
                          <p className="mt-1 text-xs text-[#8EA1A5]">{stat.detail}</p>
                        </div>
                      ))}
                    </div>

                    <section className="rounded-2xl border border-[#2F4047] bg-[#0F171D]/65 p-4">
                      <div className="mb-3 flex items-center justify-between">
                        <h2 className="ow-display text-xl text-[#F5F1E8]">
                          Installed Models
                        </h2>
                        <span className="rounded-full border border-[#355760] bg-[#15242B] px-2.5 py-1 text-xs text-[#A3BABE]">
                          {downloadedModels.length} ready
                        </span>
                      </div>

                      {downloadedModels.length === 0 ? (
                        <div className="rounded-xl border border-dashed border-[#355760] bg-[#121D24]/60 px-4 py-8 text-center">
                          <div className="mx-auto mb-3 grid h-12 w-12 place-items-center rounded-xl bg-[#16343D]">
                            <Download className="h-5 w-5 text-[#A4D4C8]" />
                          </div>
                          <p className="text-sm text-[#E8E1D5]">
                            No model installed yet.
                          </p>
                          <p className="mt-1 text-xs text-[#95A7AA]">
                            Start with `base` for balance or `large-v3-turbo` for accuracy.
                          </p>
                          <CleanButton
                            onClick={() => setCurrentView("library")}
                            className="mt-4"
                          >
                            Open Library
                          </CleanButton>
                        </div>
                      ) : (
                        <div className="space-y-3">
                          {downloadedModels.map((model) => (
                            <ModelCardGeneric
                              key={model.name}
                              model={model}
                              isActive={activeModel === model.name}
                              onAction={() => onSelectModel(model.name)}
                              actionLabel={activeModel === model.name ? "Active" : "Activate"}
                              actionDisabled={activeModel === model.name}
                            />
                          ))}
                        </div>
                      )}
                    </section>
                  </>
                )}

                {currentView === "library" && (
                  <section className="rounded-2xl border border-[#2F4047] bg-[#0F171D]/65 p-4">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                      <h2 className="ow-display text-xl text-[#F5F1E8]">Available Models</h2>
                      {activeDownload && (
                        <span className="rounded-full border border-[#3C5660] bg-[#16242B] px-2.5 py-1 text-xs text-[#AAC2C4]">
                          Downloading {activeDownloadProgress?.percent?.toFixed(0) ?? "0"}%
                        </span>
                      )}
                    </div>
                    <div className="space-y-3">
                      {libraryModels.map((model) => (
                        <ModelCardGeneric
                          key={model.name}
                          model={model}
                          isDownloading={activeDownload === model.name}
                          downloadProgress={downloadProgress[model.name]}
                          onAction={() => onDownload(model.name)}
                          actionLabel={
                            activeDownload === model.name ? "Downloading" : "Download"
                          }
                          actionDisabled={!!activeDownload}
                        />
                      ))}
                    </div>
                    {libraryModels.length === 0 && (
                      <div className="rounded-xl border border-dashed border-[#355760] bg-[#121D24]/60 px-4 py-10 text-center">
                        <p className="text-sm text-[#E8E1D5]">All available models are installed.</p>
                        <p className="mt-1 text-xs text-[#95A7AA]">
                          Switch back to Dashboard to activate another model.
                        </p>
                      </div>
                    )}
                  </section>
                )}

                {currentView === "settings" && (
                  <section className="rounded-2xl border border-[#2F4047] bg-[#0F171D]/65 p-4">
                    <h2 className="ow-display text-xl text-[#F5F1E8]">Preferences</h2>
                    <p className="mt-1 text-sm text-[#98A9AD]">
                      This panel is next on roadmap. Current controls are available through model
                      management and your OS permissions.
                    </p>

                    <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
                      <div className="rounded-xl border border-[#31434A] bg-[#111B21] p-3">
                        <p className="text-xs uppercase tracking-[0.14em] text-[#8EA2A5]">
                          Hotkey
                        </p>
                        <p className="mt-1 text-sm text-[#F2EBDD]">Hold Fn to dictate</p>
                      </div>
                      <div className="rounded-xl border border-[#31434A] bg-[#111B21] p-3">
                        <p className="text-xs uppercase tracking-[0.14em] text-[#8EA2A5]">
                          Paste Mode
                        </p>
                        <p className="mt-1 text-sm text-[#F2EBDD]">
                          Clipboard-preserving insertion
                        </p>
                      </div>
                    </div>
                  </section>
                )}
              </div>
            )}
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}

function ModelCardGeneric({
  model,
  isActive,
  isDownloading,
  downloadProgress,
  onAction,
  actionLabel,
  actionDisabled,
}: {
  model: ModelInfo;
  isActive?: boolean;
  isDownloading?: boolean;
  downloadProgress?: ModelDownloadProgressEvent;
  onAction: () => void;
  actionLabel: React.ReactNode;
  actionDisabled?: boolean;
}) {
  const percent =
    typeof downloadProgress?.percent === "number"
      ? Math.max(0, Math.min(100, downloadProgress.percent))
      : 0;
  const runtime = model.runtime ?? "whisper.cpp";
  const note = model.note ?? model.notes;

  return (
    <div
      className={`relative overflow-hidden rounded-2xl border transition-all duration-300 ${
        isActive
          ? "border-[#4A736D] bg-[linear-gradient(130deg,rgba(25,56,57,0.7)_0%,rgba(14,30,36,0.75)_65%)]"
          : "border-[#2F4047] bg-[linear-gradient(140deg,rgba(17,27,33,0.82)_0%,rgba(13,20,25,0.78)_100%)] hover:border-[#44616B]"
      }`}
    >
      <div className="relative z-10 flex flex-col gap-3 p-3.5 sm:flex-row sm:items-center sm:justify-between sm:p-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-[#F6F1E8] sm:text-base">
              {model.name}
            </h3>
            {isActive && (
              <span className="rounded-full border border-[#4A736D] bg-[#1C3A39]/80 px-2 py-[0.12rem] text-[0.64rem] uppercase tracking-[0.14em] text-[#BCE0D5]">
                Active
              </span>
            )}
            {model.distro && (
              <span className="rounded-full border border-[#3A4A51] bg-[#162228] px-2 py-[0.12rem] text-[0.64rem] uppercase tracking-[0.14em] text-[#AABABE]">
                {model.distro}
              </span>
            )}
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-[#A3B4B8]">
            <span className="inline-flex items-center gap-1 rounded-md border border-[#33444B] bg-[#141F25] px-2 py-1">
              <Download className="h-3.5 w-3.5" />
              {MODEL_SIZE_HINTS[model.name] || "Unknown size"}
            </span>
            <span className="inline-flex items-center gap-1 rounded-md border border-[#33444B] bg-[#141F25] px-2 py-1">
              <Cpu className="h-3.5 w-3.5" />
              {runtime}
            </span>
          </div>
          {note && <p className="mt-2 truncate text-xs text-[#92A3A7]">{note}</p>}
        </div>

        <CleanButton
          onClick={onAction}
          disabled={actionDisabled}
          className={`w-full sm:w-auto ${
            isActive
              ? "border-[#486C67] bg-[#1C3636] text-[#DCEDE8]"
              : isDownloading
                ? "border-[#456B75] bg-[#1B323B]"
                : ""
          }`}
        >
          {isDownloading ? (
            <span className="inline-flex items-center gap-2">
              <LoaderCircle className="h-4 w-4 animate-spin" />
              {Math.round(percent)}%
            </span>
          ) : (
            actionLabel
          )}
        </CleanButton>
      </div>

      {isDownloading && (
        <div className="absolute bottom-0 left-0 h-[2px] w-full bg-[#132027]">
          <motion.div
            initial={{ width: 0 }}
            animate={{ width: `${percent}%` }}
            className="h-full bg-gradient-to-r from-[#5FC9AE] to-[#86DDCF]"
          />
        </div>
      )}
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
