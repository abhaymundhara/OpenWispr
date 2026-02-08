import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "framer-motion";
import { theme, themeComponents } from "./theme";

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
    className={`${theme.button.primary} px-4 py-2 rounded-xl text-sm ${className || ""} ${disabled ? "opacity-50 cursor-not-allowed pointer-events-none" : ""}`}
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
  const [currentView, setCurrentView] = useState<"dashboard" | "library" | "settings">("dashboard");
  const [tab, setTab] = useState<"downloaded" | "library">("downloaded"); // For compatibility

  // Sync currentView and tab
  useEffect(() => {
    if (currentView === "dashboard") setTab("downloaded");
    if (currentView === "library") setTab("library");
  }, [currentView]);

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
      className={`group w-full flex items-center space-x-3 px-4 py-3 rounded-xl transition-all duration-300 text-left ${
        active
          ? "text-white bg-white/10 backdrop-blur-xl shadow-lg border border-white/20"
          : "text-white/60 hover:text-white hover:bg-white/5 hover:backdrop-blur-lg hover:border hover:border-white/10 border border-transparent"
      }`}
    >
      <span className={`transition-transform duration-200 group-hover:scale-110 ${active ? "text-white" : "text-white/70"}`}>
        {icon}
      </span>
      <span className="text-sm font-medium">{label}</span>
    </button>
  );

  return (
    <div className={`h-screen ${themeComponents.container} flex overflow-hidden font-sans text-white`} style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}>
      {/* Enhanced Dark Glass Sidebar */}
      <aside className={`w-72 relative flex flex-col border-r border-white/5`} style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        {/* Sophisticated dark glass background with gradient overlay */}
        <div className="absolute inset-0 bg-gradient-to-b from-gray-900/90 via-black/80 to-gray-950/95 backdrop-blur-2xl"></div>
        
        {/* Subtle inner shadow for depth */}
        <div className="absolute inset-0 shadow-[inset_-1px_0_0_rgba(255,255,255,0.05)]"></div>

        <div className="relative z-10 flex flex-col h-full">
          {/* Logo Section */}
          <div className="px-6 pt-10 pb-8">
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 bg-gradient-to-br from-white/20 to-white/5 rounded-xl flex items-center justify-center backdrop-blur-md border border-white/20 shadow-xl shadow-black/20">
                <svg viewBox="0 0 24 24" className="w-6 h-6 text-white" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z" />
                </svg>
              </div>
              <div className="flex flex-col">
                <span className={`text-white font-semibold text-lg drop-shadow-sm ${theme.typography.heading}`}>OpenWispr</span>
                <span className="text-white/40 text-xs font-medium tracking-wide">AI Assistant</span>
              </div>
            </div>
          </div>

          {/* Navigation */}
          <nav className="flex-1 px-4 space-y-2 overflow-y-auto scrollbar-hide">
            <div className="px-2 mb-2 text-xs font-semibold text-white/30 uppercase tracking-wider">Menu</div>
            <NavItem
              active={currentView === "dashboard"}
              onClick={() => setCurrentView("dashboard")}
              label="Dashboard"
              icon={<span className="material-icons-outlined text-[20px]">dashboard</span>}
            />
            <NavItem
              active={currentView === "library"}
              onClick={() => setCurrentView("library")}
              label="Model Library"
              icon={<span className="material-icons-outlined text-[20px]">library_books</span>}
            />
            {/* Spacer */}
            <div className="h-4"></div>
            <div className="px-2 mb-2 text-xs font-semibold text-white/30 uppercase tracking-wider">Settings</div>
            <NavItem
              active={currentView === "settings"}
              onClick={() => setCurrentView("settings")}
              label="Settings"
              icon={<span className="material-icons-outlined text-[20px]">settings</span>}
            />
          </nav>

          {/* Pro/Status Card */}
          <div className="p-4 mt-auto">
            <div className={`${theme.glass.secondary} rounded-xl p-4 border border-white/10`}>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-medium text-white/50">Active Engine</span>
                <span className="flex h-2 w-2 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]"></span>
              </div>
              <div className="text-sm font-medium text-white/90 truncate">{activeModelInfo?.name || "No Model Active"}</div>
              {activeModelInfo && (
                <div className="mt-2 text-[10px] text-white/40 font-mono bg-black/20 rounded px-2 py-1 inline-block">
                  {activeModelInfo.runtime}
                </div>
              )}
            </div>
          </div>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="flex-1 relative overflow-hidden bg-transparent" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        {/* Background Gradients */}
        <div className="absolute top-0 right-0 w-[600px] h-[600px] bg-indigo-500/10 rounded-full blur-[120px] pointer-events-none mix-blend-screen"></div>
        <div className="absolute bottom-0 left-0 w-[500px] h-[500px] bg-blue-500/10 rounded-full blur-[100px] pointer-events-none mix-blend-screen"></div>

        <div className="absolute inset-0 overflow-y-auto px-8 py-10">
           {/* Header */}
           <div className="mb-10">
            <h1 className={`${theme.typography.displayLarge} text-4xl mb-2 text-white`}>
              {currentView === "dashboard" && "Dashboard"}
              {currentView === "library" && "Model Library"}
              {currentView === "settings" && "Settings"}
            </h1>
            <p className={`${theme.text.secondary} text-lg font-light`}>
              {currentView === "dashboard" && "Manage your active AI models and performance."}
              {currentView === "library" && "Discover and install new speech recognition models."}
              {currentView === "settings" && "Configure application preferences."}
            </p>
          </div>

          <div className="space-y-6 max-w-5xl">
            {error && (
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                className="p-4 bg-red-500/10 border border-red-500/20 rounded-xl text-red-200 text-sm flex items-center gap-3 backdrop-blur-md"
              >
                <span className="material-icons-outlined">error_outline</span>
                {error}
              </motion.div>
            )}

            {currentView === "dashboard" && (
              <>
                 {/* Quick Stats Grid */}
                 <div className="grid grid-cols-3 gap-6 mb-8">
                  <div className={`${theme.glass.primary} p-6 rounded-2xl`}>
                    <div className="text-white/50 text-sm font-medium mb-1">Total Models</div>
                    <div className="text-3xl font-bold text-white tracking-tight">{downloadedModels.length}</div>
                  </div>
                  <div className={`${theme.glass.primary} p-6 rounded-2xl`}>
                    <div className="text-white/50 text-sm font-medium mb-1">Active Runtime</div>
                    <div className="text-3xl font-bold text-white tracking-tight">{activeModelInfo?.runtime || "-"}</div>
                  </div>
                  <div className={`${theme.glass.primary} p-6 rounded-2xl`}>
                    <div className="text-white/50 text-sm font-medium mb-1">System Status</div>
                    <div className="text-3xl font-bold text-white tracking-tight flex items-center gap-2">
                      Ready <span className="w-2 h-2 bg-emerald-400 rounded-full animate-pulse"></span>
                    </div>
                  </div>
                </div>

                <h2 className={`${theme.typography.heading} text-xl text-white mb-4`}>Installed Models</h2>
                {downloadedModels.length === 0 ? (
                  <div className={`${theme.glass.secondary} rounded-2xl p-12 text-center border-dashed border border-white/10`}>
                     <div className="w-16 h-16 bg-white/5 rounded-2xl flex items-center justify-center mx-auto mb-4">
                        <span className="material-icons-outlined text-white/20 text-3xl">download</span>
                     </div>
                     <h3 className="text-white font-medium mb-2">No models installed</h3>
                     <p className="text-white/40 text-sm mb-6">Download a model from the library to get started.</p>
                     <button 
                       onClick={() => setCurrentView("library")}
                       className={`${theme.button.primary} px-6 py-2.5 rounded-xl`}
                     >
                       Go to Library
                     </button>
                  </div>
                ) : (
                  <div className="grid gap-4">
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
              </>
            )}

            {currentView === "library" && (
              <>
                <div className="grid gap-4">
                  {libraryModels.map((model) => (
                    <ModelCardGeneric
                      key={model.name}
                      model={model}
                      isDownloading={activeDownload === model.name}
                      downloadProgress={downloadProgress[model.name]}
                      onAction={() => onDownload(model.name)}
                      actionLabel={activeDownload === model.name ? "Downloading" : "Download"}
                      actionDisabled={!!activeDownload}
                    />
                  ))}
                  {libraryModels.length === 0 && (
                    <div className="text-center py-20 text-white/40">All available models are installed.</div>
                  )}
                </div>
              </>
            )}

            {currentView === "settings" && (
              <div className={`${theme.glass.primary} p-8 rounded-2xl`}>
                <h3 className="text-lg font-medium text-white mb-4">Application Settings</h3>
                <p className="text-white/50 text-sm">Settings configuration is coming soon.</p>
              </div>
            )}
          </div>
        </div>
      </main>
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

  return (
    <div className={`group relative overflow-hidden rounded-2xl border transition-all duration-300 ${
        isActive 
          ? "bg-gradient-to-r from-indigo-500/10 to-indigo-500/5 border-indigo-500/20 shadow-lg shadow-indigo-900/10" 
          : `${theme.glass.primary}`
      }`}>
        
       {/* Active Glow/Indicator */}
       {isActive && (
          <div className="absolute left-0 top-0 bottom-0 w-1 bg-indigo-500 shadow-[0_0_10px_rgba(99,102,241,0.6)]"></div>
       )}

      <div className="relative z-10 flex items-center justify-between p-6">
        <div className="flex-1 min-w-0 pr-8">
           <div className="flex items-center gap-3 mb-1">
             <h3 className={`text-lg font-semibold tracking-tight ${isActive ? "text-white" : "text-white/90"}`}>
               {model.name}
             </h3>
             {isActive && (
               <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-indigo-500/20 text-indigo-200 border border-indigo-500/20 uppercase tracking-widest">
                 Active
               </span>
             )}
             {model.distro && (
               <span className="px-2 py-0.5 rounded text-[10px] font-medium bg-white/5 text-white/40 border border-white/5 uppercase tracking-wider">
                 {model.distro}
               </span>
             )}
           </div>
           
           <div className="flex items-center gap-4 text-xs text-white/40 mt-2 font-medium">
             <div className="flex items-center gap-1.5 bg-white/5 px-2 py-1 rounded-md">
                <span className="material-icons-outlined text-[14px]">sd_storage</span>
                {MODEL_SIZE_HINTS[model.name] || "Unknown size"}
             </div>
             <div className="flex items-center gap-1.5 bg-white/5 px-2 py-1 rounded-md">
                <span className="material-icons-outlined text-[14px]">speed</span>
                {model.runtime}
             </div>
             {model.note && (
               <span className="truncate max-w-[200px] border-l border-white/10 pl-3">{model.note}</span>
             )}
           </div>
        </div>

        <div className="flex items-center gap-4">
          <CleanButton
            onClick={onAction}
            disabled={actionDisabled}
            className={`min-w-[100px] ${isActive ? "bg-white/5 border border-white/10 text-white/50" : isDownloading ? "bg-indigo-600/20 text-indigo-200 border border-indigo-500/30" : ""}`}
          >
            {isDownloading ? (
               <div className="flex items-center justify-center gap-2">
                 <div className="w-3 h-3 border-2 border-indigo-200 border-t-transparent rounded-full animate-spin"></div>
                 <span>{Math.round(percent)}%</span>
               </div>
            ) : actionLabel}
          </CleanButton>
        </div>
      </div>

       {/* Loading Progress Bar */}
       {isDownloading && (
        <div className="absolute bottom-0 left-0 w-full h-[2px] bg-white/5">
          <motion.div
            initial={{ width: 0 }}
            animate={{ width: `${percent}%` }}
            className="h-full bg-gradient-to-r from-indigo-500 to-blue-500 shadow-[0_0_8px_rgba(99,102,241,0.8)]"
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
    // Force transparency on mount
    document.documentElement.style.backgroundColor = "transparent";
    document.body.style.backgroundColor = "transparent";
    const root = document.getElementById("root");
    if (root) root.style.backgroundColor = "transparent";

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

  if (windowLabel === "dictation_pill") {
    return <DictationPillApp />;
  }

  // Default to Dashboard layout
  return <Dashboard />;
}

export default App;
