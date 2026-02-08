import React, { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api";

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
  "tiny": "~75 MB",
  "tiny.en": "~75 MB",
  "base": "~140 MB",
  "base.en": "~140 MB",
  "small": "~460 MB",
  "small.en": "~460 MB",
  "medium": "~1.5 GB",
  "medium.en": "~1.5 GB",
  "large-v3-turbo": "~1.6 GB",
  "large-v3": "~3.1 GB",
  "mlx-community/whisper-tiny": "~75 MB",
  "mlx-community/whisper-base": "~140 MB",
  "mlx-community/whisper-small": "~460 MB",
  "mlx-community/whisper-medium": "~1.5 GB",
  "mlx-community/whisper-large-v3-turbo": "~1.6 GB",
  "mlx-community/whisper-large-v3": "~3.1 GB",
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

    const playTone = (freq: number, gainStart: number, startOffset: number, duration: number) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.frequency.setValueAtTime(freq, t + startOffset);
      gain.gain.setValueAtTime(gainStart, t + startOffset);
      gain.gain.exponentialRampToValueAtTime(0.0001, t + startOffset + duration);
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
        const variation = Math.sin(time + index * 0.4) * 0.6 + (Math.random() - 0.5) * 1.0;
        const height = Math.max(
          3,
          Math.min(maxHeight, baseHeight + normalizedLevel * (maxHeight - baseHeight) + variation),
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
      animate={{ scale: 1, opacity: 1, y: 0 }}
      exit={{ scale: 0.96, opacity: 0, y: 12 }}
      transition={{ type: "spring", damping: 26, stiffness: 340, mass: 0.9 }}
      className="fixed bottom-0 left-1/2 z-[999999] -translate-x-1/2"
      onClick={() => {
        if (shouldRecord) onStop();
      }}
    >
      <AnimatePresence mode="wait">
        {status === "processing" ? (
          <motion.div
            key="transcribing"
            initial={{ scale: 0.85, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.85, opacity: 0 }}
            className="flex h-8 w-20 items-center justify-center rounded-2xl border border-white/20 bg-[rgba(20,20,20,0.95)] px-[15px] shadow-[0_4px_12px_rgba(0,0,0,0.2)] backdrop-blur-[15px]"
          >
            <div className="flex gap-1.5">
              <span className="loading-dot" />
              <span className="loading-dot" />
              <span className="loading-dot" />
            </div>
          </motion.div>
        ) : status === "error" ? (
          <motion.div
            key="error"
            initial={{ scale: 0.85, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.85, opacity: 0 }}
            className="flex h-8 min-w-[140px] items-center justify-center rounded-2xl border border-red-300/30 bg-red-500/95 px-[15px] text-white shadow-[0_4px_12px_rgba(255,59,48,0.3)]"
          >
            <span className="mr-1.5 text-sm">⚠️</span>
            <span className="max-w-[220px] truncate text-xs font-medium">
              {error || "Transcription error"}
            </span>
          </motion.div>
        ) : (
          <motion.div
            key="recording"
            initial={{ scale: 0.85, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.85, opacity: 0 }}
            className="flex h-8 w-[120px] items-center rounded-2xl border border-white/20 bg-[rgba(20,20,20,0.95)] px-[15px] shadow-[0_4px_12px_rgba(0,0,0,0.2)] backdrop-blur-[15px]"
          >
            <JarvisWaveBars audioLevel={audioLevel} />
          </motion.div>
        )}
      </AnimatePresence>
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
    console.log("📥 Starting download for model:", model);
    setActiveDownload(model);
    setError(undefined);
    try {
      await invoke("download_model", { model });
      console.log("✅ Download completed for:", model);
      await loadModels();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      console.error("❌ Download failed:", errorMsg);
      setError(errorMsg);
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

  return (
    <div className="h-screen w-screen bg-zinc-950 text-zinc-100 p-6 overflow-auto">
      <div className="mx-auto max-w-3xl">
        <div className="flex items-end justify-between mb-6">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">Model Manager</h1>
            <p className="text-zinc-400 text-sm mt-1">
              Download Whisper models locally. These stay on-device.
            </p>
            {downloadedModels.length > 0 && (
              <div className="mt-3 p-3 rounded-lg bg-blue-500/10 border border-blue-500/20">
                <p className="text-blue-300 text-sm font-medium mb-1">
                  🎮 Hold Ctrl + Shift together to record
                </p>
                <p className="text-zinc-400 text-xs">
                  Press and hold both Ctrl and Shift keys, speak, then release either key to stop and transcribe. Check the console for key detection logs.
                </p>
              </div>
            )}
          </div>
          <button
            className="px-3 py-1.5 text-sm rounded-lg bg-zinc-800 hover:bg-zinc-700 transition-colors"
            onClick={loadModels}
            disabled={loading}
          >
            Refresh
          </button>
        </div>

        <div className="mb-4 inline-flex rounded-lg border border-zinc-800 overflow-hidden">
          <button
            className={`px-4 py-2 text-sm transition-colors ${
              tab === "downloaded"
                ? "bg-zinc-200 text-zinc-900"
                : "bg-zinc-900 text-zinc-400 hover:text-zinc-200"
            }`}
            onClick={() => setTab("downloaded")}
          >
            Downloaded
          </button>
          <button
            className={`px-4 py-2 text-sm transition-colors border-l border-zinc-800 ${
              tab === "library"
                ? "bg-zinc-200 text-zinc-900"
                : "bg-zinc-900 text-zinc-400 hover:text-zinc-200"
            }`}
            onClick={() => setTab("library")}
          >
            Library
          </button>
        </div>

        {error && (
          <div className="mb-4 rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </div>
        )}

        {tab === "downloaded" ? (
          <div className="rounded-xl border border-zinc-800 overflow-hidden">
            <div className="grid grid-cols-[1.2fr_0.8fr_0.6fr] gap-2 px-4 py-3 text-xs uppercase tracking-wide text-zinc-400 bg-zinc-900/80">
              <div>Model</div>
              <div>Runtime</div>
              <div className="text-right">Select</div>
            </div>
            {loading ? (
              <div className="px-4 py-8 text-sm text-zinc-400">Loading models...</div>
            ) : downloadedModels.length === 0 ? (
              <div className="px-4 py-8 text-sm text-zinc-400">
                No downloaded models yet. Go to Library and download one.
              </div>
            ) : (
              downloadedModels.map((model) => {
                const selected = activeModel === model.name;
                return (
                  <div
                    key={model.name}
                    className="grid grid-cols-[1.2fr_0.8fr_0.6fr] gap-2 px-4 py-3 border-t border-zinc-900 items-center"
                  >
                    <div className="font-medium">{model.name}</div>
                    <div className="text-zinc-400 text-sm">{model.runtime}</div>
                    <div className="text-right">
                      <button
                        className={`px-3 py-1.5 rounded-md text-sm transition-colors ${
                          selected
                            ? "bg-emerald-500/20 text-emerald-300"
                            : "bg-zinc-800 hover:bg-zinc-700 text-zinc-100"
                        }`}
                        onClick={() => onSelectModel(model.name)}
                        disabled={selected}
                      >
                        {selected ? "Selected" : "Use"}
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        ) : (
          <div className="rounded-xl border border-zinc-800 overflow-hidden">
            <div className="grid grid-cols-[1.2fr_0.7fr_0.8fr_0.7fr] gap-2 px-4 py-3 text-xs uppercase tracking-wide text-zinc-400 bg-zinc-900/80">
              <div>Model</div>
              <div>Size</div>
              <div>Runtime</div>
              <div className="text-right">Action</div>
            </div>
            {loading ? (
              <div className="px-4 py-8 text-sm text-zinc-400">Loading models...</div>
            ) : (
              models.map((model) => {
                const downloading = activeDownload === model.name;
                return (
                  <div
                    key={model.name}
                    className="grid grid-cols-[1.2fr_0.7fr_0.8fr_0.7fr] gap-2 px-4 py-3 border-t border-zinc-900 items-center"
                  >
                    <div>
                      <div className="font-medium">{model.name}</div>
                      {model.note && (
                        <div className="text-xs text-zinc-500 mt-0.5">{model.note}</div>
                      )}
                    </div>
                    <div className="text-zinc-400 text-sm">
                      {MODEL_SIZE_HINTS[model.name] ?? "Unknown"}
                    </div>
                    <div className="text-zinc-400 text-sm">{model.runtime}</div>
                    <div className="text-right">
                      <button
                        className={`px-3 py-1.5 rounded-md text-sm transition-colors ${
                          model.downloaded
                            ? "bg-emerald-500/20 text-emerald-300 cursor-default"
                            : !model.can_download
                              ? "bg-zinc-800 text-zinc-500 cursor-not-allowed"
                            : "bg-blue-600 hover:bg-blue-500 text-white"
                        }`}
                        disabled={model.downloaded || downloading || !model.can_download}
                        onClick={() => onDownload(model.name)}
                      >
                        {model.downloaded
                          ? "Downloaded"
                          : !model.can_download
                            ? "Coming soon"
                          : downloading
                            ? "Downloading..."
                            : "Download"}
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function DictationPillApp() {
  const [fnHeld, setFnHeld] = useState(false);
  const [sttStatus, setSttStatus] = useState<TranscriptionStatus>("idle");
  const [sttError, setSttError] = useState<string>();
  const previousFnHeld = useRef(false);
  const { playStartSound, playStopSound } = useFeedbackSounds(true);

  useEffect(() => {
    let unlistenHold: (() => void) | undefined;
    let unlistenToggle: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    const setupListener = async () => {
      try {
        unlistenHold = await listen<boolean>("fn-hold", (event) => {
          console.log("🎯 fn-hold event received:", event.payload);
          setFnHeld(event.payload);
        });
        unlistenToggle = await listen("global-shortcut-pressed", () => {
          setFnHeld((prev) => !prev);
        });
        unlistenStatus = await listen<TranscriptionStatusEvent>(
          "transcription-status",
          (event) => {
            setSttStatus(event.payload.status);
            if (event.payload.status === "listening" || event.payload.status === "idle") {
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

    return () => {
      if (unlistenHold) unlistenHold();
      if (unlistenToggle) unlistenToggle();
      if (unlistenStatus) unlistenStatus();
    };
  }, []);

  useEffect(() => {
    console.log("🔄 fnHeld state changed:", fnHeld);
    if (fnHeld && !previousFnHeld.current) {
      console.log("▶️ Starting sound");
      playStartSound();
    } else if (!fnHeld && previousFnHeld.current) {
      console.log("⏹️ Stopping sound");
      playStopSound();
    }
    previousFnHeld.current = fnHeld;
  }, [fnHeld, playStartSound, playStopSound]);

  const showPill = fnHeld || sttStatus !== "idle";
  console.log("👁️ Render state - fnHeld:", fnHeld, "sttStatus:", sttStatus, "showPill:", showPill);

  return (
    <div className="h-screen w-screen flex items-center justify-center overflow-hidden bg-transparent">
      <style>{`
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
