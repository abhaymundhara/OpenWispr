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

  SlidersHorizontal,
  Monitor,
  Keyboard,
  Languages,
  Mic,
  CheckCircle2,
  Library,
  LoaderCircle,
  Hash,
  Settings2,
  X,
} from "lucide-react";

const ShortcutKey = ({ children }: { children: React.ReactNode }) => (
  <kbd className="min-w-[20px] justify-center flex items-center h-6 rounded border border-zinc-200 bg-zinc-50 px-1.5 font-sans text-[11px] font-medium text-zinc-500">
    {children}
  </kbd>
);

type ShortcutKeyName = "push_to_talk" | "hands_free_toggle" | "command_mode";

interface ShortcutSettings {
  push_to_talk: string;
  hands_free_toggle: string;
  command_mode: string;
}

const DEFAULT_SHORTCUTS: ShortcutSettings = {
  push_to_talk: "fn",
  hands_free_toggle: "fn+space",
  command_mode: "fn+ctrl",
};

const MODIFIER_TOKENS = new Set(["fn", "ctrl", "control", "shift", "alt", "option", "meta", "cmd", "command", "win", "super"]);

const normalizeEventCodeToken = (code: string): string | null => {
  if (!code) return null;
  if (code === "Space") return "space";
  if (code === "Enter" || code === "NumpadEnter") return "enter";
  if (code === "Tab") return "tab";
  if (code === "Escape") return "escape";
  if (code === "Backspace") return "backspace";
  if (code === "ArrowUp") return "up";
  if (code === "ArrowDown") return "down";
  if (code === "ArrowLeft") return "left";
  if (code === "ArrowRight") return "right";
  if (code === "Minus" || code === "NumpadSubtract") return "-";
  if (code === "Equal" || code === "NumpadAdd") return "=";
  if (code === "Comma") return ",";
  if (code === "Period" || code === "NumpadDecimal") return ".";
  if (code === "Semicolon") return ";";
  if (code === "Quote") return "'";
  if (code === "Slash" || code === "NumpadDivide") return "/";
  if (code === "Backquote") return "`";
  if (code === "Backslash") return "\\";
  if (code === "BracketLeft") return "[";
  if (code === "BracketRight") return "]";
  if (code.startsWith("Key") && code.length === 4) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (code.startsWith("F")) {
    const maybeFn = code.slice(1);
    if (/^\d{1,2}$/.test(maybeFn)) return `f${maybeFn}`;
  }
  return null;
};

const normalizeEventKeyToken = (key: string): string | null => {
  const trimmed = key.trim();
  if (!trimmed) return null;
  const lower = trimmed.toLowerCase();
  if (lower === " ") return "space";
  if (lower === "spacebar") return "space";
  if (lower === "escape" || lower === "esc") return "escape";
  if (lower === "enter" || lower === "return") return "enter";
  if (lower === "tab") return "tab";
  if (lower === "backspace") return "backspace";
  if (lower === "arrowup") return "up";
  if (lower === "arrowdown") return "down";
  if (lower === "arrowleft") return "left";
  if (lower === "arrowright") return "right";
  if (lower.length === 1) return lower;
  return lower;
};

const keyboardEventToShortcut = (event: KeyboardEvent): string | null => {
  const tokens: string[] = [];
  const hasFn = event.getModifierState?.("Fn") || event.key.toLowerCase() === "fn";
  if (hasFn) tokens.push("fn");
  if (event.ctrlKey) tokens.push("ctrl");
  if (event.shiftKey) tokens.push("shift");
  if (event.altKey) tokens.push("alt");
  if (event.metaKey) tokens.push("meta");

  const keyToken = normalizeEventCodeToken(event.code) || normalizeEventKeyToken(event.key);
  if (keyToken && !MODIFIER_TOKENS.has(keyToken)) {
    tokens.push(keyToken);
  }

  if (tokens.length === 0) return null;
  return tokens.join("+");
};

const formatShortcutPart = (part: string) => {
  const key = part.trim().toLowerCase();
  if (key === "fn") return "fn";
  if (key === "control") return "Ctrl";
  if (key === "ctrl") return "Ctrl";
  if (key === "meta") return "Meta";
  if (key === "command") return "Cmd";
  if (key === "cmd") return "Cmd";
  if (key === "alt") return "Alt";
  if (key === "option") return "Option";
  if (key === "space") return "Space";
  if (key === "enter") return "Enter";
  if (key === "tab") return "Tab";
  if (key === "escape") return "Esc";
  if (key === "backspace") return "Backspace";
  if (key === "up") return "Up";
  if (key === "down") return "Down";
  if (key === "left") return "Left";
  if (key === "right") return "Right";
  return key.toUpperCase();
};

const shortcutToKeys = (shortcut: string) =>
  shortcut
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map(formatShortcutPart);

const shortcutToLabel = (shortcut: string) => shortcutToKeys(shortcut).join(" + ");

const ShortcutRow = ({
  title,
  description,
  keys,
  actionLabel,
  onAction,
  actionDisabled,
}: {
  title: string;
  description: string;
  keys: string[];
  actionLabel: string;
  onAction: () => void;
  actionDisabled?: boolean;
}) => (
  <div className="flex items-center justify-between rounded-xl border border-zinc-100 bg-zinc-50/50 p-4">
    <div>
      <h4 className="text-sm font-medium text-zinc-900">{title}</h4>
      <p className="mt-0.5 text-xs text-zinc-500">{description}</p>
    </div>
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-2 py-1.5 shadow-sm">
        {keys.map((k, i) => (
          <ShortcutKey key={i}>{k}</ShortcutKey>
        ))}
      </div>
      <button
        onClick={onAction}
        disabled={actionDisabled}
        className="inline-flex h-8 min-w-[5.5rem] items-center justify-center rounded-lg bg-zinc-100 px-3 text-[0.82rem] font-medium text-zinc-900 transition-colors hover:bg-zinc-200 disabled:cursor-not-allowed disabled:opacity-55"
      >
        {actionLabel}
      </button>
    </div>
  </div>
);

const ShortcutsModal = ({
  shortcuts,
  onStartRecording,
  recordingField,
  onResetDefaults,
  saving,
  error,
  onClose,
}: {
  shortcuts: ShortcutSettings;
  onStartRecording: (field: ShortcutKeyName) => void;
  recordingField: ShortcutKeyName | null;
  onResetDefaults: () => void;
  saving: boolean;
  error?: string;
  onClose: () => void;
}) => (
  <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/20 p-4 backdrop-blur-sm" onClick={onClose}>
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={{ duration: 0.1 }}
      className="w-full max-w-[520px] overflow-hidden rounded-2xl bg-white shadow-xl ring-1 ring-black/5"
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex items-center justify-between border-b border-zinc-100 px-6 py-4">
        <h3 className="text-lg font-semibold text-zinc-900">
          Keyboard shortcuts
        </h3>
        <button
          onClick={onClose}
          className="rounded-full p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600"
        >
          <X className="h-5 w-5" />
        </button>
      </div>
      
      <div className="p-6 space-y-4">
        <ShortcutRow
          title="Push to talk"
          description="Hold to say something short"
          keys={shortcutToKeys(shortcuts.push_to_talk)}
          actionLabel={recordingField === "push_to_talk" ? "Listening..." : "Record"}
          onAction={() => onStartRecording("push_to_talk")}
          actionDisabled={saving}
        />
        <ShortcutRow
          title="Hands-free mode"
          description="Press to start and stop dictation"
          keys={shortcutToKeys(shortcuts.hands_free_toggle)}
          actionLabel={recordingField === "hands_free_toggle" ? "Listening..." : "Record"}
          onAction={() => onStartRecording("hands_free_toggle")}
          actionDisabled={saving}
        />
        <ShortcutRow
          title="Command mode"
          description="Reserved for command actions"
          keys={shortcutToKeys(shortcuts.command_mode)}
          actionLabel={recordingField === "command_mode" ? "Listening..." : "Record"}
          onAction={() => onStartRecording("command_mode")}
          actionDisabled={saving}
        />

        {recordingField && (
          <p className="rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs font-medium text-zinc-600">
            Press your shortcut now. Press Esc to cancel.
          </p>
        )}

        {error && (
          <p className="rounded-lg border border-red-100 bg-red-50 px-3 py-2 text-xs font-medium text-red-700">
            {error}
          </p>
        )}

        <div className="pt-4 flex justify-center">
            <button
              onClick={onResetDefaults}
              disabled={saving}
              className="text-sm font-medium text-zinc-500 hover:text-zinc-900 disabled:cursor-not-allowed disabled:opacity-55"
            >
                Reset to default
            </button>
        </div>
      </div>
    </motion.div>
  </div>
);

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

interface AnalyticsStats {
  lifetime_removed_sec: number;
  sessions_count: number;
  day_streak: number;
  last_session_date: string | null;
  total_words: number;
  total_seconds: number;
}

interface AudioDevice {
  id: string;
  name: string;
}

interface Settings {
  input_device: string | null;
  language: string | null;
  local_transcription_enabled: boolean;
  llm_provider: string | null;
  ollama_base_url: string | null;
  ollama_model: string | null;
  system_llm_model: string | null;
  text_formatting_enabled: boolean;
  text_formatting_mode: string;
  shortcuts: {
    push_to_talk: string;
    hands_free_toggle: string;
    command_mode: string;
  };
}

interface OllamaModel {
  name: string;
  size: number;
  digest: string;
  details: {
    format: string;
    family: string;
    parameter_size: string;
    quantization_level: string;
  };
}

interface LlmModelInfo {
  name: string;
  size_mb: number;
  downloaded: boolean;
  description: string;
  hf_repo: string;
  filename: string;
}

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
  "sherpa-onnx/parakeet-tdt-0.6b-v2-int8": "~600 MB",
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
  | "transcription"
  | "system"
  | "models";

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

// --- Light Theme Settings Components ---

const LightSettingsNavItem = ({
  active,
  icon,
  label,
  onClick,
}: {
  active?: boolean;
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
}) => (
  <button
    onClick={onClick}
    className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-[0.92rem] font-medium transition-colors ${
      active
        ? "bg-white text-zinc-900 shadow-sm ring-1 ring-zinc-200"
        : "text-zinc-500 hover:bg-zinc-200/50 hover:text-zinc-700"
    }`}
  >
    <span className={active ? "text-zinc-900" : "text-zinc-400"}>{icon}</span>
    <span>{label}</span>
  </button>
);

const LightSettingsRow = ({
  title,
  description,
  actionLabel,
  onAction,
  className,
}: {
  title: string;
  description: string;
  actionLabel?: React.ReactNode;
  onAction?: () => void;
  className?: string;
}) => (
  <div
    className={`flex items-start justify-between py-5 ${className ?? "border-b border-zinc-100 last:border-0"}`}
  >
    <div className="pr-4">
      <h4 className="text-[0.95rem] font-semibold text-zinc-900">{title}</h4>
      <p className="mt-0.5 text-[0.85rem] leading-relaxed text-zinc-500">
        {description}
      </p>
    </div>
    {actionLabel && (
      <button
        onClick={onAction}
        className="inline-flex h-8 min-w-[5rem] items-center justify-center rounded-lg bg-zinc-100 px-3 text-[0.85rem] font-medium text-zinc-900 transition-colors hover:bg-zinc-200"
      >
        {actionLabel}
      </button>
    )}
  </div>
);

const LightTranscriptionSettings = ({
  models,
  activeModel,
  onSelectModel,
  enabled,
  onToggleEnabled,
}: {
  models: ModelInfo[];
  activeModel?: string;
  onSelectModel: (model: string) => void;
  enabled: boolean;
  onToggleEnabled: (enabled: boolean) => void;
}) => {

  return (
    <div className="py-5 border-b border-zinc-100 last:border-0">
      <div className="flex items-center gap-2 mb-3">
        <div className="p-1.5 rounded-md bg-purple-100/50 text-purple-600">
          <Mic className="h-4 w-4" />
        </div>
        <h4 className="text-[0.95rem] font-semibold text-zinc-900">Local Transcription</h4>
        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[0.65rem] font-medium bg-zinc-100 text-zinc-500 uppercase tracking-wide">
          Offline
        </span>
      </div>

      <div className="flex items-start justify-between mb-4 pl-[34px]">
         <div className="pr-4">
             <h5 className="text-[0.9rem] font-medium text-zinc-900">Enable Local Transcription</h5>
             <p className="mt-0.5 text-[0.8rem] text-zinc-500 leading-relaxed">
               100% private, works offline. Select a model below.
             </p>
         </div>
         <button 
           onClick={() => onToggleEnabled(!enabled)}
           className={`relative inline-flex h-5 w-9 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${enabled ? 'bg-zinc-900' : 'bg-zinc-200'}`}
         >
           <span className={`pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${enabled ? 'translate-x-4' : 'translate-x-0'}`} />
         </button>
      </div>

      <div className={`pl-[34px] transition-all duration-200 ${enabled ? 'opacity-100' : 'opacity-40 pointer-events-none grayscale'}`}>
          <div className="relative">
            <select
              value={activeModel}
              onChange={(e) => onSelectModel(e.target.value)}
              className="block w-full appearance-none rounded-lg border border-zinc-200 bg-zinc-50 py-2 pl-3 pr-8 text-sm text-zinc-900 focus:border-zinc-900 focus:outline-none focus:ring-1 focus:ring-zinc-900 sm:text-sm"
            >
              <option value="" disabled>Select a model</option>
              {models.map((model) => (
                <option key={model.name} value={model.name}>
                  {model.name} ({MODEL_SIZE_HINTS[model.name] || "Unknown size"}) {model.downloaded ? "- Ready" : "- Download Needed"}
                </option>
              ))}
            </select>
            <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-zinc-500">
               <ArrowRight className="h-4 w-4 rotate-90" />
            </div>
          </div>
          
          {activeModel && models.find(m => m.name === activeModel)?.downloaded && (
             <p className="mt-2 flex items-center text-xs text-emerald-600 font-medium">
               <CheckCircle2 className="mr-1.5 h-3.5 w-3.5" />
               Model ready to use
             </p>
          )}
           <p className="mt-2 text-xs text-zinc-400">
             Whisper models are general-purpose. Parakeet models offer higher accuracy but may be larger.
           </p>
      </div>
    </div>
  );
};

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
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [formattingOpen, setFormattingOpen] = useState(false);
  const [shortcutError, setShortcutError] = useState<string>();
  const [savingShortcuts, setSavingShortcuts] = useState(false);
  const [recordingShortcut, setRecordingShortcut] = useState<ShortcutKeyName | null>(null);
  
  // Real Data State
  const [analytics, setAnalytics] = useState<AnalyticsStats | null>(null);
  const [inputDevices, setInputDevices] = useState<AudioDevice[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);

  // LLM State
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [loadingOllama, setLoadingOllama] = useState(false);
  
  // System LLM State (SmolLM2)
  const [systemLlmModels, setSystemLlmModels] = useState<LlmModelInfo[]>([]);
  const [systemLlmDownloadProgress, setSystemLlmDownloadProgress] = useState<Record<string, ModelDownloadProgressEvent>>({});

  const fetchOllamaModels = async (baseUrl: string) => {
      setLoadingOllama(true);
      try {
          const models = await invoke<OllamaModel[]>("get_ollama_models", { baseUrl });
          setOllamaModels(models);
          // Auto-select first if none selected
          if (!settings?.ollama_model && models.length > 0) {
              const first = models[0].name;
              await updateLlmSettings(settings?.llm_provider || "ollama", baseUrl, first);
          }
      } catch (e) {
          setError("Failed to fetch Ollama models: " + e);
          setOllamaModels([]); // Clear on error
      } finally {
          setLoadingOllama(false);
      }
  };

  const updateLlmSettings = async (provider: string, baseUrl: string, model: string) => {
      await invoke("set_llm_settings", { provider, baseUrl, model });
      setSettings(prev => prev ? ({ ...prev, llm_provider: provider, ollama_base_url: baseUrl, ollama_model: model }) : null);
  };

  const fetchSystemLlmModels = async () => {
      try {
          const models = await invoke<LlmModelInfo[]>("list_llm_models");
          setSystemLlmModels(models);
      } catch (e) {
          setError("Failed to fetch system LLM models: " + e);
      }
  };

  const downloadSystemLlmModel = async (modelName: string) => {
      try {
          await invoke("download_llm_model", { model: modelName });
          // Reload models after download
          await fetchSystemLlmModels();
      } catch (err) {
          setError(err instanceof Error ? err.message : String(err));
      }
  };

  const activateSystemLlmModel = async (modelName: string) => {
      try {
          await invoke("set_active_llm_model", { model: modelName });
          setSettings(prev => prev ? ({ ...prev, system_llm_model: modelName }) : null);
      } catch (err) {
          setError(err instanceof Error ? err.message : String(err));
      }
  };

  const onSettingsOpen = () => {
      if (settings?.ollama_base_url) {
          void fetchOllamaModels(settings.ollama_base_url);
      }
      // Load system LLM models when settings open
      void fetchSystemLlmModels();
  };

  const loadData = async () => {
    setLoading(true);
    setError(undefined);
    try {
      const [modelsData, selectedModel, analyticsData, devicesData, settingsData, systemModelsData] = await Promise.all([
        invoke<ModelInfo[]>("list_models"),
        invoke<string>("get_active_model"),
        invoke<AnalyticsStats>("get_analytics_stats"),
        invoke<AudioDevice[]>("list_input_devices"),
        invoke<Settings>("get_settings"),
        invoke<LlmModelInfo[]>("list_llm_models"),
      ]);
      setModels(modelsData);
      setActiveModel(selectedModel);
      setAnalytics(analyticsData);
      setInputDevices(devicesData);
      setSettings(settingsData);
      setSystemLlmModels(systemModelsData);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadData();
  }, []);



  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;
    let unlistenLlmProgress: (() => void) | undefined;
    const setup = async () => {
      try {
        // Listen for model download progress
        unlisten = await listen<ModelDownloadProgressEvent>(
          "model-download-progress",
          (event) => {
            const progress = event.payload;
            setDownloadProgress((prev) => ({
              ...prev,
              [progress.model]: progress,
            }));
          }
        );
        
        // Listen for LLM model download progress
        unlistenLlmProgress = await listen<ModelDownloadProgressEvent>(
          "llm-model-download-progress",
          (event) => {
            const progress = event.payload;
            setSystemLlmDownloadProgress((prev) => ({
              ...prev,
              [progress.model]: progress,
            }));
            // Reload models when download completes
            if (progress.done && !progress.error) {
              void fetchSystemLlmModels();
            }
          }
        );

        // Listen for analytics updates
        unlistenProgress = await listen<AnalyticsStats>(
          "analytics-update",
          (event) => {
            setAnalytics(event.payload);
          }
        );
      } catch (e) {
        console.error("Failed to setup event listeners", e);
      }
    };
    void setup();
    return () => {
      if (unlisten) unlisten();
      if (unlistenProgress) unlistenProgress();
      if (unlistenLlmProgress) unlistenLlmProgress();
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
      // Reload models to update status
      const [modelsData, selectedModel] = await Promise.all([
        invoke<ModelInfo[]>("list_models"),
        invoke<string>("get_active_model"),
      ]);
      setModels(modelsData);
      setActiveModel(selectedModel);
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

  const persistShortcuts = async (next: ShortcutSettings) => {
    setSavingShortcuts(true);
    setShortcutError(undefined);
    try {
      const updated = await invoke<ShortcutSettings>("set_shortcuts", {
        pushToTalk: next.push_to_talk,
        handsFreeToggle: next.hands_free_toggle,
        commandMode: next.command_mode,
      });
      setSettings((prev) => (prev ? { ...prev, shortcuts: updated } : prev));
    } catch (err) {
      setShortcutError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingShortcuts(false);
    }
  };

  const beginShortcutRecording = (field: ShortcutKeyName) => {
    if (savingShortcuts) return;
    setShortcutError(undefined);
    setRecordingShortcut(field);
  };

  const resetShortcutsToDefault = () => {
    void persistShortcuts(DEFAULT_SHORTCUTS);
  };

  useEffect(() => {
    if (!shortcutsOpen || !recordingShortcut) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setRecordingShortcut(null);
        return;
      }

      const recorded = keyboardEventToShortcut(event);
      if (!recorded) return;
      event.preventDefault();
      event.stopPropagation();

      const nextShortcuts: ShortcutSettings = {
        ...(settings?.shortcuts ?? DEFAULT_SHORTCUTS),
        [recordingShortcut]: recorded,
      };
      void persistShortcuts(nextShortcuts);
      setRecordingShortcut(null);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [shortcutsOpen, recordingShortcut, settings?.shortcuts]);

  const downloadedModels = models.filter((m) => m.downloaded);
  const activeModelInfo = models.find((m) => m.name === activeModel);
  const libraryModels = models.filter((m) => m.can_download);
  const installableModels = models.filter(
    (m) => m.can_download && !m.downloaded,
  );
  const selectedSectionTitle =
    section === "general"
      ? "General"
      : section === "transcription"
        ? "Transcription"
        : section === "system"
          ? "System"
          : "Model Library";

  const sectionSummary =
    section === "general"
      ? "Core dictation controls and defaults."
      : section === "transcription"
        ? "Manage offline transcription models."
        : section === "system"
          ? "Desktop behavior and app-level options."
          : "Configure LLM provider and models.";
  const shortcuts = settings?.shortcuts ?? DEFAULT_SHORTCUTS;

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
                  onSettingsOpen();
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
                        {analytics ? (analytics.lifetime_removed_sec / 60).toFixed(0) : "0"} mins
                      </span>
                      <span className="text-lg text-white/58">
                        lifetime saved
                      </span>
                    </div>
                    <div className="mt-2 text-sm text-white/40">
                      {analytics?.sessions_count || 0} sessions • all time
                    </div>
                  </div>
                  <div className="flex h-2 w-2 rounded-full bg-white/20"></div>
                </div>
              </div>
            </div>

            <div className="mt-8 grid grid-cols-3 gap-12 px-2">
               <div className="text-center">
                 <div className="text-4xl font-semibold tracking-tight text-white/96">
                   {analytics?.day_streak || 0}
                 </div>
                 <div className="mt-1 text-sm font-medium text-white/58">
                   Day Streak
                 </div>
               </div>
               <div className="text-center">
                 <div className="text-4xl font-semibold tracking-tight text-white/96">
                   {analytics && analytics.total_seconds > 0 ? (analytics.total_words / (analytics.total_seconds / 60)).toFixed(0) : "0"}
                 </div>
                 <div className="mt-1 text-sm font-medium text-white/58">
                   Avg WPM
                 </div>
               </div>
               <div className="text-center">
                 <div className="text-4xl font-semibold tracking-tight text-white/96">
                   {analytics?.total_words || 0}
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
              className="absolute inset-0 z-50 flex items-center justify-center bg-black/20 p-4 backdrop-blur-sm"
              onClick={() => setSettingsOpen(false)}
            >
              <motion.div
                initial={{ opacity: 0, scale: 0.95 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.95 }}
                transition={{ duration: 0.1 }}
                className="flex h-[85vh] max-h-[700px] w-full max-w-[960px] overflow-hidden rounded-2xl bg-white shadow-2xl ring-1 ring-black/5"
                onClick={(e) => e.stopPropagation()}
              >
                {/* Sidebar */}
                <aside className="w-[260px] flex-none bg-zinc-50/80 px-4 py-5 backdrop-blur-xl">
                  <div className="mb-6 px-2">
                    <p className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
                      Settings
                    </p>
                    <div className="mt-2 space-y-0.5">
                      <LightSettingsNavItem
                        active={section === "general"}
                        icon={<SlidersHorizontal className="h-4 w-4" />}
                        label="General"
                        onClick={() => setSection("general")}
                      />
                      <LightSettingsNavItem
                        active={section === "transcription"}
                        icon={<Mic className="h-4 w-4" />}
                        label="Transcription"
                        onClick={() => setSection("transcription")}
                      />
                      <LightSettingsNavItem
                        active={section === "system"}
                        icon={<Monitor className="h-4 w-4" />}
                        label="System"
                        onClick={() => setSection("system")}
                      />
                      <LightSettingsNavItem
                        active={section === "models"}
                        icon={<Hash className="h-4 w-4" />}
                        label="Models"
                        onClick={() => setSection("models")}
                      />
                    </div>
                  </div>


                </aside>

                {/* Content */}
                <main className="flex-1 overflow-y-auto bg-white px-8 py-8">
                  <div className="mx-auto max-w-2xl">
                    <h2 className="mb-6 text-2xl font-semibold text-zinc-900">
                      {selectedSectionTitle}
                    </h2>

                    {section === "general" && (
                      <div className="divide-y divide-zinc-100">
                        <LightSettingsRow
                          title="Keyboard shortcuts"
                          description={`Push to talk: ${shortcutToLabel(shortcuts.push_to_talk)}  |  Hands-free: ${shortcutToLabel(shortcuts.hands_free_toggle)}`}
                          actionLabel="Change"
                          onAction={() => {
                            setShortcutError(undefined);
                            setRecordingShortcut(null);
                            setShortcutsOpen(true);
                          }}
                        />
                        <LightSettingsRow
                          title="Microphone"
                          description={settings?.input_device || "Default System Device"}
                          actionLabel="Change"
                          onAction={async () => {
                             // Simple cycle for now
                             if (inputDevices.length === 0) return;
                             const currentIndex = inputDevices.findIndex(d => d.name === settings?.input_device);
                             const nextIndex = (currentIndex + 1) % inputDevices.length;
                             const nextDevice = inputDevices[nextIndex];
                             await invoke("set_input_device", { deviceId: nextDevice.id });
                             setSettings(prev => prev ? ({ ...prev, input_device: nextDevice.name }) : null);
                          }}
                        />
                        <LightSettingsRow
                          title="Languages"
                          description={settings?.language === "en" ? "English" : settings?.language === "hi" ? "Hindi" : "Auto"}
                          actionLabel="Change"
                          onAction={async () => {
                             const langs = ["en", "hi", "auto"];
                             const current = settings?.language || "en";
                             const currentIndex = langs.indexOf(current);
                             const next = langs[(currentIndex + 1) % langs.length];
                             await invoke("set_language", { language: next });
                             setSettings(prev => prev ? ({ ...prev, language: next }) : null);
                          }}
                         />
                        <LightSettingsRow
                          title="Text formatting"
                          description={
                            settings.text_formatting_enabled
                              ? `Enabled · ${settings.text_formatting_mode.charAt(0).toUpperCase() + settings.text_formatting_mode.slice(1)}`
                              : "Disabled"
                          }
                          actionLabel="Configure"
                          onAction={() => setFormattingOpen(true)}
                        />
                      </div>
                    )}

                    {section === "transcription" && (
                         <LightTranscriptionSettings
                          models={libraryModels.filter(m => 
                            ["tiny", "tiny.en", "base", "base.en", "small", "small.en", "sherpa-onnx/parakeet-tdt-0.6b-v2-int8"].includes(m.name)
                          )}
                          activeModel={activeModel}
                          enabled={settings?.local_transcription_enabled ?? true}
                          onToggleEnabled={async (enabled) => {
                              await invoke("set_transcription_enabled", { enabled });
                              setSettings(prev => prev ? ({ ...prev, local_transcription_enabled: enabled }) : null);
                          }}
                          onSelectModel={(model) => {
                             const m = models.find(x => x.name === model);
                             if (m && !m.downloaded) {
                                void onDownload(model);
                             } else {
                                void onSelectModel(model);
                             }
                          }}
                        />
                    )}

                    {section === "models" && (
                       <div className="space-y-6">
                           {/* Provider Selection */}
                           <div className="space-y-3">
                               <label className="text-sm font-medium text-zinc-900">LLM Provider</label>
                                <div className="relative">
                                    <select
                                        value={settings?.llm_provider || "system"}
                                        onChange={(e) => updateLlmSettings(e.target.value, settings?.ollama_base_url || "http://localhost:11434", settings?.ollama_model || "")}
                                        className="w-full appearance-none rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-900 focus:border-zinc-900 focus:outline-none focus:ring-1 focus:ring-zinc-900"
                                    >
                                        <option value="system">System (Local)</option>
                                        <option value="ollama">Ollama</option>
                                    </select>
                                    <ArrowRight className="absolute right-3 top-3 h-4 w-4 rotate-90 text-zinc-400 pointer-events-none" />
                                </div>
                           </div>

                           {/* Ollama Configuration */}
                           {settings?.llm_provider === "ollama" && (
                               <div className="space-y-6 animate-in fade-in slide-in-from-top-2 duration-200">
                                   <div className="space-y-3">
                                       <label className="text-sm font-medium text-zinc-900">Ollama Base URL</label>
                                       <div className="flex gap-2">
                                           <input
                                               type="text"
                                               value={settings?.ollama_base_url || ""}
                                               onChange={(e) => {
                                                   const val = e.target.value;
                                                   setSettings(prev => prev ? ({ ...prev, ollama_base_url: val }) : null);
                                               }}
                                               onBlur={() => updateLlmSettings("ollama", settings?.ollama_base_url || "http://localhost:11434", settings?.ollama_model || "")}
                                               placeholder="http://localhost:11434"
                                               className="flex-1 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-900 focus:border-zinc-900 focus:outline-none focus:ring-1 focus:ring-zinc-900"
                                           />
                                            <button 
                                               onClick={() => fetchOllamaModels(settings?.ollama_base_url || "http://localhost:11434")}
                                               disabled={loadingOllama}
                                               className="rounded-lg bg-zinc-100 px-4 text-sm font-medium text-zinc-900 hover:bg-zinc-200 disabled:opacity-50"
                                            >
                                                {loadingOllama ? <LoaderCircle className="h-4 w-4 animate-spin" /> : "Refresh"}
                                            </button>
                                       </div>
                                       <p className="text-xs text-zinc-500">Default: http://localhost:11434</p>
                                   </div>

                                   <div className="space-y-3">
                                       <label className="text-sm font-medium text-zinc-900">Model</label>
                                       <div className="relative">
                                           <select
                                               value={settings?.ollama_model || ""}
                                               onChange={(e) => updateLlmSettings("ollama", settings?.ollama_base_url || "", e.target.value)}
                                               className="w-full appearance-none rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-900 focus:border-zinc-900 focus:outline-none focus:ring-1 focus:ring-zinc-900"
                                           >
                                               <option value="" disabled>Select a model</option>
                                               {ollamaModels.map(m => (
                                                   <option key={m.digest} value={m.name}>
                                                       {m.name} ({(m.size / 1024 / 1024 / 1024).toFixed(1)} GB)
                                                   </option>
                                               ))}
                                           </select>
                                           <ArrowRight className="absolute right-3 top-3 h-4 w-4 rotate-90 text-zinc-400 pointer-events-none" />
                                       </div>
                                       {ollamaModels.length === 0 && !loadingOllama && (
                                           <p className="text-xs text-amber-600 flex items-center mt-2">
                                               No models found. Ensure Ollama is running.
                                           </p>
                                       )}
                                    </div>
                                </div>
                            )}

                            {/* System (Local) Configuration */}
                            {settings?.llm_provider === "system" && (
                                <div className="space-y-6 animate-in fade-in slide-in-from-top-2 duration-200">
                                    <div className="space-y-4">
                                        {systemLlmModels.map((model) => (
                                            <div key={model.name} className="rounded-xl border border-zinc-100 bg-zinc-50/50 p-4">
                                                <div className="flex items-start justify-between">
                                                    <div className="flex-1">
                                                        <h4 className="text-sm font-semibold text-zinc-900">{model.name}</h4>
                                                        <p className="mt-1 text-xs text-zinc-500">Quantized GGUF format</p>
                                                        <p className="mt-1 text-xs text-zinc-400">Size: {model.size_mb} MB</p>
                                                    </div>
                                                    <div>
                                                        {model.downloaded ? (
                                                            <button
                                                                onClick={() => activateSystemLlmModel(model.name)}
                                                                disabled={settings?.system_llm_model === model.name}
                                                                className="rounded-lg bg-emerald-100 px-3 py-2 text-xs font-medium text-emerald-900 hover:bg-emerald-200 disabled:opacity-50 disabled:cursor-not-allowed"
                                                            >
                                                                {settings?.system_llm_model === model.name ? "Active" : "Activate"}
                                                            </button>
                                                        ) : (
                                                            <button
                                                                onClick={() => downloadSystemLlmModel(model.name)}
                                                                disabled={systemLlmDownloadProgress[model.name]?.done === false}
                                                                className="rounded-lg bg-zinc-900 px-3 py-2 text-xs font-medium text-white hover:bg-zinc-800 disabled:opacity-50 disabled:cursor-not-allowed"
                                                            >
                                                                {systemLlmDownloadProgress[model.name]?.done === false ? "Downloading..." : "Download"}
                                                            </button>
                                                        )}
                                                    </div>
                                                </div>
                                                {/* Download progress indicator */}
                                                {systemLlmDownloadProgress[model.name] && !systemLlmDownloadProgress[model.name].done && (
                                                    <div className="mt-3">
                                                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-200">
                                                            <div
                                                                className="h-full bg-zinc-900 transition-all duration-300"
                                                                style={{ width: `${systemLlmDownloadProgress[model.name].percent || 0}%` }}
                                                            />
                                                        </div>
                                                        <p className="mt-1 text-xs text-zinc-500">
                                                            {systemLlmDownloadProgress[model.name].stage}... {Math.round(systemLlmDownloadProgress[model.name].percent || 0)}%
                                                        </p>
                                                    </div>
                                                )}
                                            </div>
                                        ))}
                                    </div>
                                    <p className="text-xs text-zinc-500">
                                        💡 System models run locally with hardware acceleration (Metal/CUDA). No internet required after download.
                                    </p>
                                </div>
                            )}
                        </div>
                     )}


                  </div>
                </main>
              </motion.div>
            </div>
          )}
          
          <AnimatePresence>
            {shortcutsOpen && (
                <ShortcutsModal
                  shortcuts={shortcuts}
                  onStartRecording={beginShortcutRecording}
                  recordingField={recordingShortcut}
                  onResetDefaults={resetShortcutsToDefault}
                  saving={savingShortcuts}
                  error={shortcutError}
                  onClose={() => {
                    setRecordingShortcut(null);
                    setShortcutsOpen(false);
                  }}
                />
            )}
          </AnimatePresence>
        </main>
      </div>

      {/* Text Formatting Modal */}
      {formattingOpen && (
        <div
          className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 backdrop-blur-sm"
          onClick={() => setFormattingOpen(false)}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="w-full max-w-md rounded-2xl bg-white p-6 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-4 text-lg font-semibold text-zinc-900">
              Text Formatting
            </h3>
            <p className="mb-6 text-sm text-zinc-600">
              AI-powered cleanup of transcribed speech using local LLM
            </p>

            <div className="space-y-4">
              {/* Enable Toggle */}
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-zinc-700">
                  Enable formatting
                </span>
                <button
                  onClick={async () => {
                    const newValue = !settings?.text_formatting_enabled;
                    await invoke("set_formatting_settings", {
                      enabled: newValue,
                      mode: settings?.text_formatting_mode || "standard",
                    });
                    setSettings((prev) =>
                      prev
                        ? { ...prev, text_formatting_enabled: newValue }
                        : null
                    );
                  }}
                  className={`relative h-6 w-11 rounded-full transition-colors ${
                    settings?.text_formatting_enabled
                      ? "bg-indigo-600"
                      : "bg-zinc-300"
                  }`}
                >
                  <span
                    className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform ${
                      settings?.text_formatting_enabled
                        ? "translate-x-5"
                        : "translate-x-0.5"
                    }`}
                  />
                </button>
              </div>

              {/* Mode Selector */}
              {settings?.text_formatting_enabled && (
                <div className="space-y-2">
                  <label className="text-sm font-medium text-zinc-700">
                    Formatting mode
                  </label>
                  <select
                    value={settings.text_formatting_mode}
                    onChange={async (e) => {
                      const mode = e.target.value;
                      await invoke("set_formatting_settings", {
                        enabled: settings.text_formatting_enabled,
                        mode,
                      });
                      setSettings((prev) =>
                        prev ? { ...prev, text_formatting_mode: mode } : null
                      );
                    }}
                    className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                  >
                    <option value="quick">Quick (Filler + Punctuation)</option>
                    <option value="standard">
                      Standard (+ Capitalization)
                    </option>
                    <option value="smart">Smart (+ Numbers/Dates)</option>
                  </select>
                  <p className="text-xs text-zinc-500">
                    {settings.text_formatting_mode === "quick" &&
                      "Removes filler words and adds basic punctuation"}
                    {settings.text_formatting_mode === "standard" &&
                      "Quick + proper capitalization"}
                    {settings.text_formatting_mode === "smart" &&
                      "Standard + smart number/date formatting"}
                  </p>
                </div>
              )}
            </div>

            <div className="mt-6 flex justify-end">
              <button
                onClick={() => setFormattingOpen(false)}
                className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
              >
                Done
              </button>
            </div>
          </motion.div>
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
  visible,
  shouldRecord,
  status,
  error,
  onStop,
}: {
  visible: boolean;
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
      initial={false}
      animate={
        visible
          ? { scale: 1, opacity: 1, y: 0 }
          : { scale: 0.98, opacity: 0, y: 10 }
      }
      transition={{ type: "spring", damping: 30, stiffness: 380, mass: 0.8 }}
      className="fixed bottom-2 left-1/2 z-[999999] -translate-x-1/2"
      style={{ pointerEvents: visible ? "auto" : "none" }}
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
  const HOLD_RELEASE_UI_DEBOUNCE_MS = 90;
  const PILL_HIDE_DEBOUNCE_MS = 120;
  const [fnHeldRaw, setFnHeldRaw] = useState(false);
  const [fnHeld, setFnHeld] = useState(false);
  const [showPill, setShowPill] = useState(false);
  const [sttStatus, setSttStatus] = useState<TranscriptionStatus>("idle");
  const [sttError, setSttError] = useState<string>();
  const previousFnHeld = useRef(false);
  const holdReleaseTimerRef = useRef<number | null>(null);
  const hidePillTimerRef = useRef<number | null>(null);
  const { playStartSound, playStopSound } = useFeedbackSounds(true);

  useEffect(() => {
    if (holdReleaseTimerRef.current !== null) {
      window.clearTimeout(holdReleaseTimerRef.current);
      holdReleaseTimerRef.current = null;
    }

    if (fnHeldRaw) {
      setFnHeld(true);
      return;
    }

    holdReleaseTimerRef.current = window.setTimeout(() => {
      setFnHeld(false);
      holdReleaseTimerRef.current = null;
    }, HOLD_RELEASE_UI_DEBOUNCE_MS);

    return () => {
      if (holdReleaseTimerRef.current !== null) {
        window.clearTimeout(holdReleaseTimerRef.current);
        holdReleaseTimerRef.current = null;
      }
    };
  }, [fnHeldRaw]);

  const shouldShowPill = fnHeld || sttStatus !== "idle";
  useEffect(() => {
    if (hidePillTimerRef.current !== null) {
      window.clearTimeout(hidePillTimerRef.current);
      hidePillTimerRef.current = null;
    }

    if (shouldShowPill) {
      setShowPill(true);
      return;
    }

    hidePillTimerRef.current = window.setTimeout(() => {
      setShowPill(false);
      hidePillTimerRef.current = null;
    }, PILL_HIDE_DEBOUNCE_MS);

    return () => {
      if (hidePillTimerRef.current !== null) {
        window.clearTimeout(hidePillTimerRef.current);
        hidePillTimerRef.current = null;
      }
    };
  }, [shouldShowPill]);

  useEffect(() => {
    let unlistenHold: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    const setupListener = async () => {
      try {
        unlistenHold = await listen<boolean>("fn-hold", (event) => {
          setFnHeldRaw(event.payload);
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
    return () => {
      if (unlistenHold) unlistenHold();
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
      <FloatingPill
        visible={showPill}
        shouldRecord={fnHeld}
        status={sttStatus}
        error={sttError}
        onStop={() => {
          playStopSound();
          setFnHeldRaw(false);
          setFnHeld(false);
          invoke("stop_recording").catch(console.error);
        }}
      />
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
