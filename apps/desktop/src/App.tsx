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

function ModelManager() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [activeDownload, setActiveDownload] = useState<string>();
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

  const onDownload = async (model: string) => {
    setActiveDownload(model);
    setError(undefined);
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

  // Reusable Components
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
      className={`relative px-5 py-2 text-sm font-medium transition-colors duration-200 ${
        active ? "text-white" : "text-white/40 hover:text-white/70"
      }`}
    >
      {active && (
        <motion.div
          layoutId="activeTab"
          className="absolute inset-0 bg-white/10 rounded-full"
          transition={{ type: "spring", stiffness: 500, damping: 30 }}
        />
      )}
      <span className="relative z-10">{children}</span>
    </button>
  );

  const StatusBadge = ({ type }: { type: "whisper" | "stt" | string }) => {
    const isWhisper = type.toLowerCase().includes("whisper");
    return (
      <span
        className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium tracking-wide border ${
          isWhisper
            ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-400"
            : "border-blue-500/20 bg-blue-500/10 text-blue-400"
        }`}
      >
        {type.toUpperCase()}
      </span>
    );
  };

  return (
    <div className="h-screen w-screen bg-[#050505] text-white/90 p-8 overflow-hidden flex flex-col font-sans select-none">
      {/* Header */}
      <div className="flex items-center justify-between mb-8 flex-shrink-0">
        <div>
          <h1 className="text-2xl font-light tracking-tight text-white mb-1">
            Models
          </h1>
          <p className="text-white/40 text-sm font-light">
            Manage your local speech engines
          </p>
        </div>
        <button
          onClick={loadModels}
          disabled={loading}
          className="p-2 rounded-full text-white/40 hover:text-white hover:bg-white/5 transition-all active:scale-95"
          title="Refresh"
        >
          <svg
            className={`w-5 h-5 ${loading ? "animate-spin" : ""}`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
        </button>
      </div>

      {/* Tabs */}
      <div className="flex justify-center mb-8 flex-shrink-0">
        <div className="bg-white/5 p-1 rounded-full inline-flex">
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

      {/* Error Message */}
      <AnimatePresence>
        {error && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="mb-6 mx-auto max-w-lg w-full bg-red-500/10 border border-red-500/20 text-red-200 px-4 py-3 rounded-xl text-sm text-center backdrop-blur-sm"
          >
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Content Area */}
      <div className="flex-1 overflow-y-auto min-h-0 pr-2 -mr-2 scrollbar-hide">
        <div className="max-w-3xl mx-auto space-y-3 pb-8">
          {loading && models.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-20 text-white/20">
              <span className="loading-dot mb-4" />
              <span className="text-sm font-light">Fetching models...</span>
            </div>
          ) : tab === "downloaded" ? (
            downloadedModels.length === 0 ? (
              <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                className="text-center py-20"
              >
                <div className="text-white/20 mb-3 text-4xl">box</div>
                <p className="text-white/40 font-light">No models installed</p>
                <button
                  onClick={() => setTab("library")}
                  className="mt-4 text-emerald-400 hover:text-emerald-300 text-sm hover:underline underline-offset-4"
                >
                  Go to Library →
                </button>
              </motion.div>
            ) : (
              <AnimatePresence mode="popLayout">
                {downloadedModels.map((model) => {
                  const isSelected = activeModel === model.name;
                  return (
                    <motion.div
                      layout
                      key={model.name}
                      initial={{ opacity: 0, scale: 0.98 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.98 }}
                      className={`group relative flex items-center justify-between p-5 rounded-2xl border transition-all duration-300 ${
                        isSelected
                          ? "bg-white/[0.08] border-white/20 shadow-[0_4px_24px_-8px_rgba(255,255,255,0.1)]"
                          : "bg-white/[0.03] border-white/[0.06] hover:bg-white/[0.05] hover:border-white/10"
                      }`}
                    >
                      <div className="flex flex-col gap-1">
                        <div className="flex items-center gap-3">
                          <span className="text-base font-medium text-white/90">
                            {model.name}
                          </span>
                          {isSelected && (
                            <span className="flex h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.6)]" />
                          )}
                        </div>
                        <div className="flex items-center gap-3 text-xs text-white/40">
                          <span>{model.runtime}</span>
                          <span>•</span>
                          <span>
                            {MODEL_SIZE_HINTS[model.name] ?? "Unknown size"}
                          </span>
                        </div>
                      </div>

                      <button
                        onClick={() => onSelectModel(model.name)}
                        disabled={isSelected}
                        className={`px-5 py-2 rounded-full text-xs font-medium tracking-wide transition-all duration-200 ${
                          isSelected
                            ? "bg-white/10 text-white/50 cursor-default"
                            : "bg-white text-black hover:bg-white/90 hover:shadow-[0_0_12px_rgba(255,255,255,0.3)] active:scale-95"
                        }`}
                      >
                        {isSelected ? "Active" : "Select"}
                      </button>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            )
          ) : (
            <div className="space-y-3">
              {models.map((model, idx) => {
                const isDownloading = activeDownload === model.name;
                const isDownloaded = model.downloaded;
                const canDownload = model.can_download;

                return (
                  <motion.div
                    initial={{ opacity: 0, y: 10 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: idx * 0.03 }}
                    key={model.name}
                    className="group bg-white/[0.02] hover:bg-white/[0.04] border border-white/[0.04] hover:border-white/10 p-5 rounded-2xl flex items-center justify-between transition-colors duration-200"
                  >
                    <div>
                      <div className="flex items-center gap-2 mb-1">
                        <span className="text-sm font-medium text-white/80 group-hover:text-white transition-colors">
                          {model.name}
                        </span>
                        <StatusBadge type={model.runtime} />
                      </div>
                      <div className="text-xs text-white/40 font-light flex gap-3">
                        <span>{MODEL_SIZE_HINTS[model.name] || "-- MB"}</span>
                        {model.note && (
                          <>
                            <span className="opacity-30">|</span>
                            <span>{model.note}</span>
                          </>
                        )}
                      </div>
                    </div>

                    <div>
                      {isDownloaded ? (
                        <div className="px-4 py-2 text-xs font-medium text-emerald-400/80 bg-emerald-500/5 rounded-full border border-emerald-500/10 flex items-center gap-1.5">
                          <svg
                            className="w-3 h-3"
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                          >
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth={2}
                              d="M5 13l4 4L19 7"
                            />
                          </svg>
                          Installed
                        </div>
                      ) : (
                        <button
                          onClick={() => onDownload(model.name)}
                          disabled={isDownloading || !canDownload}
                          className={`relative px-5 py-2 rounded-full text-xs font-medium transition-all duration-200 ${
                            !canDownload
                              ? "bg-white/5 text-white/20 cursor-not-allowed"
                              : isDownloading
                                ? "bg-white/10 text-white/80 cursor-wait pl-9"
                                : "bg-white/10 text-white hover:bg-white/20 hover:text-white border border-white/5"
                          }`}
                        >
                          {isDownloading && (
                            <span className="absolute left-3 top-1/2 -translate-y-1/2">
                              <svg
                                className="w-3 h-3 animate-spin text-white/60"
                                fill="none"
                                viewBox="0 0 24 24"
                              >
                                <circle
                                  className="opacity-25"
                                  cx="12"
                                  cy="12"
                                  r="10"
                                  stroke="currentColor"
                                  strokeWidth="4"
                                ></circle>
                                <path
                                  className="opacity-75"
                                  fill="currentColor"
                                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                ></path>
                              </svg>
                            </span>
                          )}
                          {isDownloading
                            ? "Downloading..."
                            : !canDownload
                              ? "Unavailable"
                              : "Download"}
                        </button>
                      )}
                    </div>
                  </motion.div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* Footer / Status Bar */}
      <div className="border-t border-white/5 pt-4 mt-2 text-[10px] text-white/20 flex justify-between uppercase tracking-wider font-medium">
        <span>OpenWispr Desktop Alpha</span>
        <span>Local Processing Only</span>
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
