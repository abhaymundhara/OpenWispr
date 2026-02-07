import React, { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Mic,
  Check,
  ArrowRight,
  Settings as SettingsIcon,
  X,
} from "lucide-react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

// --- Utils ---
function cn(...inputs: (string | undefined | null | false)[]) {
  return twMerge(clsx(inputs));
}

// --- Components ---

const FloatingPill = ({ isActive }: { isActive: boolean }) => {
  return (
    <motion.div
      initial={{ y: 50, opacity: 0 }}
      animate={{ y: 0, opacity: 1 }}
      exit={{ y: 50, opacity: 0 }}
      className="fixed bottom-12 left-1/2 -translate-x-1/2 flex items-center gap-3 px-4 py-3 bg-black/60 backdrop-blur-xl border border-white/10 rounded-full shadow-2xl text-white"
    >
      <div className="relative flex items-center justify-center w-8 h-8 rounded-full bg-indigo-500">
        <Mic size={16} className="text-white z-10" />
        {isActive && (
          <motion.div
            animate={{ scale: [1, 1.5, 1], opacity: [0.5, 0, 0.5] }}
            transition={{ repeat: Infinity, duration: 2 }}
            className="absolute inset-0 bg-indigo-500 rounded-full"
          />
        )}
      </div>

      <div className="flex flex-col">
        <span className="text-sm font-medium">Listening...</span>
        <span className="text-xs text-white/50">Say "Stop" or press Esc</span>
      </div>

      <div className="h-4 w-px bg-white/20 mx-1" />

      <button className="p-1.5 hover:bg-white/10 rounded-full transition-colors">
        <SettingsIcon size={16} className="text-white/70" />
      </button>
    </motion.div>
  );
};

const OnboardingStep = ({
  title,
  description,
  action,
  onNext,
}: {
  title: string;
  description: string;
  action: React.ReactNode;
  onNext: () => void;
}) => (
  <motion.div
    initial={{ x: 20, opacity: 0 }}
    animate={{ x: 0, opacity: 1 }}
    exit={{ x: -20, opacity: 0 }}
    className="flex flex-col items-center text-center max-w-md mx-auto p-8 bg-zinc-900/90 backdrop-blur-xl border border-white/10 rounded-2xl shadow-2xl"
  >
    <h2 className="text-2xl font-bold text-white mb-2">{title}</h2>
    <p className="text-zinc-400 mb-8">{description}</p>

    <div className="mb-8 w-full flex justify-center">{action}</div>

    <button
      onClick={onNext}
      className="group flex items-center gap-2 px-6 py-2.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded-full font-medium transition-all"
    >
      Continue{" "}
      <ArrowRight
        size={16}
        className="group-hover:translate-x-0.5 transition-transform"
      />
    </button>
  </motion.div>
);

const Onboarding = ({ onComplete }: { onComplete: () => void }) => {
  const [step, setStep] = useState(0);

  const steps = [
    {
      title: "Welcome to OpenWispr",
      description:
        "The privacy-first voice dictation tool that works everywhere.",
      action: (
        <div className="w-24 h-24 bg-gradient-to-tr from-indigo-500 to-purple-500 rounded-2xl shadow-lg flex items-center justify-center">
          <Mic size={40} className="text-white" />
        </div>
      ),
    },
    {
      title: "Grant Permissions",
      description:
        "We need access to your microphone and accessibility features to type for you.",
      action: (
        <div className="space-y-3 w-full max-w-xs">
          <div className="flex items-center justify-between p-3 bg-white/5 rounded-lg border border-white/10">
            <span className="text-white text-sm">Microphone</span>
            <Check size={18} className="text-green-400" />
          </div>
          <div className="flex items-center justify-between p-3 bg-white/5 rounded-lg border border-white/10">
            <span className="text-white text-sm">Accessibility</span>
            <button className="text-xs bg-white/10 hover:bg-white/20 px-2 py-1 rounded text-white transition-colors">
              Grant
            </button>
          </div>
        </div>
      ),
    },
    {
      title: "Master the Hotkey",
      description: "Press Ctrl+Space to start dictating anywhere.",
      action: (
        <div className="flex gap-2 justify-center">
          {["Ctrl", "Space"].map((k) => (
            <div
              key={k}
              className="w-12 h-12 flex items-center justify-center bg-white/10 rounded-lg border-b-4 border-white/5 text-white font-bold"
            >
              {k}
            </div>
          ))}
        </div>
      ),
    },
  ];

  const handleNext = () => {
    if (step < steps.length - 1) {
      setStep(step + 1);
    } else {
      onComplete();
    }
  };

  return (
    <div className="fixed inset-0 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <AnimatePresence mode="wait">
        <OnboardingStep key={step} {...steps[step]} onNext={handleNext} />
      </AnimatePresence>
    </div>
  );
};

// --- Main App ---

function App() {
  const [hasCompletedOnboarding, setHasCompletedOnboarding] = useState(false);
  const [isListening, setIsListening] = useState(false);

  // Global hotkey listener
  React.useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen("global-shortcut-pressed", () => {
          setIsListening((prev) => !prev);
        });
      } catch (e) {
        console.error("Tauri event listener failed", e);
      }
    };
    setupListener();

    const handleKeyDown = (e: KeyboardEvent) => {
      // Keep backtick for dev
      if (e.key === "`") {
        setIsListening((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      if (unlisten) unlisten();
    };
  }, []);

  if (!hasCompletedOnboarding) {
    return <Onboarding onComplete={() => setHasCompletedOnboarding(true)} />;
  }

  return (
    <div className="h-screen w-screen flex items-center justify-center overflow-hidden">
      {/* 
        This is a debug overlay to help user understand hidden state
        Remove in production
      */}
      <div className="fixed top-4 left-4 text-white/30 text-xs">
        Press ` (backtick) to toggle listening <br />
        State: {isListening ? "Listening" : "Idle"}
      </div>

      <AnimatePresence>
        {isListening && <FloatingPill isActive={true} />}
      </AnimatePresence>
    </div>
  );
}

export default App;
