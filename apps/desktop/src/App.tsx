import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DictationPillApp from "./components/DictationPillApp";
import { AnimatePresence, motion } from "framer-motion";
import { theme, themeComponents } from "./theme";

// --- Types ---

interface ModelInfo {
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
    className={`inline-flex h-9 items-center justify-center rounded-xl px-4 text-[13px] font-medium transition-all duration-300 focus:outline-none focus:ring-2 focus:ring-white/20 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-30 ${
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

  const NavItem = ({
    active,
    onClick,
    icon,
    label,
    badge,
  }: {
    active: boolean;
    onClick: () => void;
    icon: React.ReactNode;
    label: string;
    badge?: number;
  }) => (
    <button
      onClick={onClick}
      className={`group w-full flex items-center justify-between px-4 py-3 rounded-xl transition-all duration-300 ${
        active
          ? "text-white bg-white/10 backdrop-blur-xl shadow-lg border border-white/20"
          : "text-white/60 hover:text-white hover:bg-white/5"
      }`}
    >
      <div className="flex items-center space-x-3">
        <span className={`transition-transform duration-300 group-hover:scale-110 ${active ? 'text-white' : 'text-white/40 group-hover:text-white/70'}`}>
          {icon}
        </span>
        <span className="text-[14px] font-medium">{label}</span>
      </div>
      {typeof badge === 'number' && badge > 0 && (
        <span className={`text-[10px] font-semibold px-2 py-0.5 rounded-full ${active ? 'bg-white/20 text-white' : 'bg-white/5 text-white/40 group-hover:text-white/60'}`}>
          {badge}
        </span>
      )}
    </button>
  );

  return (
    <div className="flex h-screen w-screen bg-gradient-to-br from-gray-900 via-gray-950 to-black text-white selection:bg-white/20 overflow-hidden font-sans">
      {/* Liquid Glass Sidebar */}
      <aside className="w-64 relative flex flex-col border-r border-white/5">
        <div className="absolute inset-0 bg-black/40 backdrop-blur-2xl"></div>
        
        <div className="relative z-10 flex flex-col h-full pt-10 pb-6 px-6">
          {/* Logo Section */}
          <div className="flex items-center space-x-3 mb-10 px-2">
            <div className="w-10 h-10 bg-gradient-to-br from-white/20 to-white/5 rounded-xl flex items-center justify-center backdrop-blur-md border border-white/20 shadow-xl shadow-black/20">
              <svg viewBox="0 0 24 24" className="w-6 h-6 text-white" fill="none" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z" />
              </svg>
            </div>
            <div className="flex flex-col">
              <span className="text-[16px] font-bold tracking-tight">OpenWispr</span>
              <span className="text-[11px] text-white/40 font-medium tracking-wide">Desktop Alpha</span>
            </div>
          </div>

          {/* Navigation */}
          <nav className="flex-1 space-y-2">
            <NavItem
              active={tab === "downloaded"}
              onClick={() => setTab("downloaded")}
              label="Installed"
              badge={downloadedModels.length}
              icon={
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                </svg>
              }
            />
            <NavItem
              active={tab === "library"}
              onClick={() => setTab("library")}
              label="Library"
              badge={libraryModels.length}
              icon={
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
              }
            />
          </nav>

          {/* Footer Info */}
          <div className="mt-auto pt-6 border-t border-white/5">
            <div className="p-4 bg-white/5 rounded-2xl border border-white/5">
              <div className="flex items-center justify-between mb-2">
                <span className="text-[11px] text-white/40 font-medium">Active Model</span>
                <span className="w-2 h-2 bg-emerald-500 rounded-full shadow-[0_0_8px_rgba(16,185,129,0.5)]"></span>
              </div>
              <p className="text-[13px] font-medium text-white/80 truncate">
                {activeModelInfo?.name || "None"}
              </p>
            </div>
          </div>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="flex-1 overflow-y-auto scrollbar-hide relative">
        {/* Subtle Radial Gradient Glows */}
        <div className="absolute top-0 right-0 w-[500px] h-[500px] bg-indigo-500/10 rounded-full blur-[120px] pointer-events-none"></div>
        <div className="absolute bottom-0 left-0 w-[400px] h-[400px] bg-blue-500/5 rounded-full blur-[100px] pointer-events-none"></div>

        <div className="relative z-10 max-w-4xl mx-auto px-10 py-12">
          {/* Header */}
          <header className="mb-12">
            <h2 className="text-[32px] font-bold tracking-tight mb-2 text-white">
              {tab === "downloaded" ? "Your Voice Models" : "Model Library"}
            </h2>
            <p className="text-[15px] text-white/50 font-medium">
              {tab === "downloaded" 
                ? "Manage and switch between your installed speech recognition engines."
                : "Browse and download high-quality local models for offline use."}
            </p>
          </header>

          {/* Error Banner */}
          <AnimatePresence>
            {error && (
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -10 }}
                className="mb-8 p-4 bg-red-500/10 border border-red-500/20 rounded-2xl flex items-start space-x-3 backdrop-blur-md"
              >
                <svg className="w-5 h-5 text-red-500 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <p className="text-[14px] text-red-200/80 font-medium leading-relaxed">{error}</p>
              </motion.div>
            )}
          </AnimatePresence>

          {/* Model List */}
          <div className="space-y-4">
            {loading && models.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-40">
                <div className="w-8 h-8 border-2 border-white/10 border-t-white/60 rounded-full animate-spin mb-4"></div>
                <span className="text-white/40 text-[14px] font-medium tracking-wide">Fetching models...</span>
              </div>
            ) : tab === "downloaded" ? (
              downloadedModels.length === 0 ? (
                <div className="py-32 flex flex-col items-center border border-dashed border-white/10 rounded-3xl bg-white/[0.02]">
                  <div className="w-16 h-16 bg-white/5 rounded-2xl flex items-center justify-center mb-6">
                    <svg className="w-8 h-8 text-white/20" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                    </svg>
                  </div>
                  <h3 className="text-white/60 text-[16px] font-semibold mb-2">No models installed</h3>
                  <button
                    onClick={() => setTab("library")}
                    className="text-indigo-400 hover:text-indigo-300 font-semibold text-[14px] transition-colors"
                  >
                    Explore the Library &rarr;
                  </button>
                </div>
              ) : (
                downloadedModels.map((model) => (
                  <ModelCardRefined
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
              <div className="py-32 text-center text-white/30 font-medium">All available models have been installed.</div>
            ) : (
              libraryModels.map((model) => (
                <ModelCardRefined
                  key={model.name}
                  model={model}
                  isDownloading={activeDownload === model.name}
                  downloadProgress={downloadProgress[model.name]}
                  onAction={() => onDownload(model.name)}
                  actionLabel={activeDownload === model.name ? "Downloading" : "Download"}
                  actionDisabled={!!activeDownload}
                />
              ))
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

function ModelCardRefined({
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

  return (
    <div
      className={`group relative overflow-hidden rounded-2xl transition-all duration-300 ${
        isActive
          ? "bg-white/10 border-white/20 shadow-2xl shadow-indigo-500/10"
          : "bg-white/5 border-white/5 hover:bg-white/[0.08] hover:border-white/10"
      } border`}
    >
      <div className="relative z-10 flex items-center justify-between p-6">
        <div className="min-w-0 flex-1 pr-6">
          <div className="flex items-center space-x-3 mb-2">
            <h3 className={`text-[17px] font-bold tracking-tight ${isActive ? "text-white" : "text-white/90"}`}>
              {model.name}
            </h3>
            {isActive && (
              <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded-full bg-indigo-500/20 border border-indigo-500/30">
                <div className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse"></div>
                <span className="text-[10px] font-bold text-indigo-300 tracking-wider">ACTIVE</span>
              </div>
            )}
            {model.note && (
              <span className="text-[11px] font-medium text-white/30 truncate max-w-[150px]">{model.note}</span>
            )}
          </div>
          
          <div className="flex items-center space-x-4 text-[13px] text-white/40 font-medium">
            <div className="flex items-center space-x-1.5">
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2 1 3 3 3h10c2 0 3-1 3-3V7c0-2-1-3-3-3H7c-2 0-3 1-3 3zM9 11h6m-6 4h3" />
              </svg>
              <span>{MODEL_SIZE_HINTS[model.name] || "Search sizes"}</span>
            </div>
            <div className="flex items-center space-x-1.5">
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              <span>{model.runtime}</span>
            </div>
          </div>
        </div>

        <div className="shrink-0">
          <CleanButton
            onClick={onAction}
            disabled={actionDisabled}
            className={`
              ${isActive
                ? "bg-white/10 text-white/60 cursor-default"
                : isDownloading
                ? "bg-indigo-500/20 text-indigo-300"
                : "bg-white/10 text-white hover:bg-white/20 border border-white/5"}
            `}
          >
            {isDownloading ? (
              <div className="flex items-center space-x-2">
                <div className="w-3 h-3 border-2 border-indigo-300/30 border-t-indigo-300 rounded-full animate-spin"></div>
                <span className="font-bold tabular-nums">{Math.round(percent)}%</span>
              </div>
            ) : (
              actionLabel
            )}
          </CleanButton>
        </div>
      </div>

      {/* Modern Slim Loader Bar */}
      {isDownloading && (
        <div className="absolute bottom-0 left-0 w-full h-1 bg-white/5">
          <motion.div
            initial={{ width: 0 }}
            animate={{ width: `${percent}%` }}
            className="h-full bg-gradient-to-r from-indigo-500 to-blue-400 shadow-[0_0_12px_rgba(99,102,241,0.6)]"
          />
        </div>
      )}
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
