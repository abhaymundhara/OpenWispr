import React, { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api";
import { appWindow } from "@tauri-apps/api/window";

type TranscriptionStatus = "idle" | "listening" | "processing" | "error";

type TranscriptionStatusEvent = {
  status: TranscriptionStatus;
  error?: string;
};

type ModelInfo = {
  name: string;
  runtime: string;
  downloaded: boolean;
  can_download: boolean;
  note?: string;
};

type ModelDownloadProgressEvent = {
  model: string;
  stage: string;
  downloaded_bytes: number;
  total_bytes?: number;
  percent?: number;
  done: boolean;
  error?: string;
  message?: string;
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
};

const windowLabel =
  (window as { __TAURI_METADATA__?: { __currentWindow?: { label?: string } } })
    .__TAURI_METADATA__?.__currentWindow?.label ?? "main";

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
    className={`inline-flex h-8 items-center justify-center rounded-lg px-3 text-[13px] font-medium transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-white/10 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 ${
      className || ""
    }`}
  >
    {children}
  </button>
);

function ModelManager() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [activeDownload, setActiveDownload] = useState<string>();
  const [downloadProgress, setDownloadProgress] = useState<
    Record<string, ModelDownloadProgressEvent>
  >({});
  const [activeModel, setActiveModel] = useState<string>();
  const [tab, setTab] = useState<"downloaded" | "library">("downloaded");

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
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setActiveDownload(undefined);
      await loadModels();
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

  // Tab Component
  const TabButton = ({
    active,
    onClick,
    children,
  }: {
    active: boolean;
    onClick: () => void;
    children: React.ReactNode;
  }) => (
    <button
      onClick={onClick}
      className={`relative px-4 py-1.5 text-[13px] font-medium transition-colors duration-300 ${
        active ? "text-white" : "text-white/40 hover:text-white/60"
      }`}
    >
      {active && (
        <motion.div
          layoutId="activeTabIndicator"
          className="absolute inset-0 -z-10 rounded-full bg-white/10"
          transition={{ type: "spring", bounce: 0.2, duration: 0.6 }}
        />
      )}
      {children}
    </button>
  );

  return (
    <div className="flex h-screen w-screen flex-col bg-[#030303] text-[#EDEDED] selection:bg-indigo-500/20">
      {/* Subtle Background Gradients */}
      <div className="pointer-events-none fixed inset-0 z-0 overflow-hidden">
        <div className="absolute -left-[20%] -top-[20%] h-[600px] w-[600px] rounded-full bg-indigo-500/5 blur-[120px]" />
        <div className="absolute -bottom-[20%] -right-[20%] h-[600px] w-[600px] rounded-full bg-blue-500/5 blur-[120px]" />
      </div>

      {/* Header Area */}
      <div className="relative z-10 border-b border-white/[0.04] bg-[#030303]/80 backdrop-blur-xl">
        <div className="mx-auto flex max-w-3xl items-center justify-between px-6 py-4">
          <div className="flex items-center gap-4">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-white/10 to-white/5 shadow-inner ring-1 ring-white/10">
              <svg
                className="h-5 w-5 text-white/90"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
                />
              </svg>
            </div>
            <div>
              <h1 className="text-[15px] font-medium leading-none tracking-tight text-white/90">
                Voice Models
              </h1>
              <p className="mt-1 text-[13px] text-white/40">
                Local speech recognition engines
              </p>
            </div>
          </div>

          <div className="flex items-center gap-1 rounded-full bg-white/[0.03] p-1 ring-1 ring-white/[0.04]">
            <TabButton
              active={tab === "downloaded"}
              onClick={() => setTab("downloaded")}
            >
              Installed
            </TabButton>
            <TabButton
              active={tab === "library"}
              onClick={() => setTab("library")}
            >
              Library
            </TabButton>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="relative z-10 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-6 py-8">
          {error && (
            <motion.div
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              className="mb-6 flex items-start gap-3 rounded-xl border border-red-500/10 bg-red-500/5 p-4 text-red-200/90 shadow-lg shadow-red-500/5"
            >
              <svg className="mt-0.5 h-4 w-4 shrink-0 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <p className="text-[13px] leading-relaxed">{error}</p>
            </motion.div>
          )}

          <div className="space-y-3">
            {loading && models.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-32 text-white/20">
                <span className="loading-dot mb-4 h-1.5 w-1.5 rounded-full bg-current" />
                <span className="text-[13px]">Loading models...</span>
              </div>
            ) : tab === "downloaded" ? (
              downloadedModels.length === 0 ? (
                <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-white/5 bg-white/[0.01] py-32 text-center">
                  <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-white/[0.03]">
                    <svg className="h-6 w-6 text-white/20" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 12H4" />
                    </svg>
                  </div>
                  <p className="text-[14px] font-medium text-white/50">No models installed</p>
                  <button onClick={() => setTab("library")} className="mt-4 text-[13px] font-medium text-indigo-400 hover:text-indigo-300 transition-colors">
                    Browse Library &rarr;
                  </button>
                </div>
              ) : (
                downloadedModels.map((model) => (
                  <ModelCard
                    key={model.name}
                    model={model}
                    isActive={activeModel === model.name}
                    onAction={() => onSelectModel(model.name)}
                    actionLabel={activeModel === model.name ? "Active" : "Activate"}
                    actionDisabled={activeModel === model.name}
                  />
                ))
              )
            ) : libraryModels.length === 0 ? (
              <div className="py-20 text-center text-[13px] text-white/30">
                No models available.
              </div>
            ) : (
              libraryModels.map((model) => (
                <ModelCard
                  key={model.name}
                  model={model}
                  isDownloaded={model.downloaded}
                  isActive={activeModel === model.name}
                  isDownloading={activeDownload === model.name}
                  downloadProgress={downloadProgress[model.name]}
                  onAction={() =>
                    model.downloaded
                      ? void onSelectModel(model.name)
                      : void onDownload(model.name)
                  }
                  actionLabel={
                    activeModel === model.name
                      ? "Active"
                      : model.downloaded
                        ? "Activate"
                        : activeDownload === model.name
                          ? "Downloading"
                          : "Download"
                  }
                  actionDisabled={!!activeDownload || activeModel === model.name}
                />
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function ModelCard({
  model,
  isDownloaded,
  isActive,
  isDownloading,
  downloadProgress,
  onAction,
  actionLabel,
  actionDisabled,
}: {
  model: ModelInfo;
  isDownloaded?: boolean;
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

  return (
    <div
      className={`group relative overflow-hidden rounded-xl border transition-all duration-300 ${
        isActive
          ? "border-indigo-500/20 bg-indigo-500/[0.04]"
          : "border-white/[0.03] bg-white/[0.01] hover:border-white/[0.08] hover:bg-white/[0.03]"
      }`}
    >
      {/* Slim Progress Bar at Bottom */}
      {isDownloading && (
        <div className="absolute bottom-0 left-0 h-[2px] w-full bg-white/5">
          <motion.div
            initial={{ width: 0 }}
            animate={{ width: `${percent}%` }}
            className="h-full bg-indigo-400 shadow-[0_0_10px_rgba(129,140,248,0.5)]"
          />
        </div>
      )}

      <div className="relative z-10 flex items-center justify-between p-5">
        <div className="min-w-0 flex-1 pr-6">
          <div className="flex items-center gap-3">
            <h3 className={`truncate text-[15px] font-medium tracking-tight ${isActive ? "text-white" : "text-white/80"}`}>
              {model.name}
            </h3>
            {isActive && (
              <div className="flex items-center gap-1.5 rounded-full border border-indigo-500/20 bg-indigo-500/10 px-2 py-0.5">
                <div className="h-1.5 w-1.5 rounded-full bg-indigo-400 shadow-[0_0_6px_rgba(129,140,248,0.8)]" />
                <span className="text-[10px] font-medium text-indigo-200">ACTIVE</span>
              </div>
            )}
            {!isActive && isDownloaded && (
              <div className="flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5">
                <div className="h-1.5 w-1.5 rounded-full bg-emerald-400 shadow-[0_0_6px_rgba(74,222,128,0.8)]" />
                <span className="text-[10px] font-medium text-emerald-200">DOWNLOADED</span>
              </div>
            )}
          </div>
          
          <div className="mt-2 flex items-center gap-4 text-[13px] text-white/40">
            <span className="flex items-center gap-1.5">
              <span>{MODEL_SIZE_HINTS[model.name] || "Unknown size"}</span>
            </span>
            <span className="h-1 w-1 rounded-full bg-white/10" />
            <span className="flex items-center gap-1.5">
              <span>{model.runtime}</span>
            </span>
            {isDownloading && (
              <>
                 <span className="h-1 w-1 rounded-full bg-white/10" />
                 <span className="text-indigo-300/90 font-medium">
                  {Math.round(percent)}%
                 </span>
              </>
            )}
          </div>
        </div>

        <div className="shrink-0">
          <CleanButton
            onClick={onAction}
            disabled={actionDisabled}
            className={
              isActive
                ? "cursor-default text-indigo-300/50"
                : actionDisabled && !isDownloading
                  ? "cursor-not-allowed bg-transparent text-white/10"
                  : "bg-white/5 text-white/70 hover:bg-white/10 hover:text-white"
            }
          >
            {isDownloading ? (
               <div className="h-4 w-4 animate-spin rounded-full border-2 border-white/20 border-t-white/80" />
            ) : (
              actionLabel
            )}
          </CleanButton>
        </div>
      </div>
    </div>
  );
}

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

            // NOTE: Do NOT hide the window here. The main window is transparent
            // and ignores cursor events, so it's effectively invisible when the
            // pill is not shown. Hiding it causes macOS to deactivate our
            // Accessory-policy app, which can terminate the process.
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
    // Force transparency on mount
    document.documentElement.style.backgroundColor = "transparent";
    document.body.style.backgroundColor = "transparent";
    const root = document.getElementById("root");
    if (root) root.style.backgroundColor = "transparent";

    // Also remove any potential shadows or backgrounds from the window itself logic via class manipulation if needed
    // But mainly targeting root elements.

    if (fnHeld && !previousFnHeld.current) {
      playStartSound();
    } else if (!fnHeld && previousFnHeld.current) {
      playStopSound();
    }
    previousFnHeld.current = fnHeld;
  }, [fnHeld, playStartSound, playStopSound]);

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
  if (windowLabel === "models") {
    return <ModelManager />;
  }
  return <DictationPillApp />;
}

export default App;
