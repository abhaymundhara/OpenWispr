import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api";

const FloatingPill = ({ isActive }: { isActive: boolean }) => {
  const [audioLevel, setAudioLevel] = useState(0);

  useEffect(() => {
    if (!isActive) {
      setAudioLevel(0);
      invoke("stop_recording").catch(console.error);
      return;
    }

    // Start audio recording
    invoke("start_recording").catch(console.error);

    // Listen for audio level events
    const unlisten = listen<number>("audio-level", (event) => {
      setAudioLevel(event.payload);
    });

    return () => {
      invoke("stop_recording").catch(console.error);
      unlisten.then((fn) => fn());
    };
  }, [isActive]);

  return (
    <motion.div
      initial={{ scale: 0.8, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      exit={{ scale: 0.8, opacity: 0 }}
      transition={{ type: "spring", damping: 20, stiffness: 300 }}
      className="fixed bottom-8 left-1/2 -translate-x-1/2"
    >
      <div className="relative flex items-center gap-2.5 px-4 py-2.5 bg-black/95 backdrop-blur-xl border border-white/20 rounded-full shadow-2xl">
        {/* Stop button */}
        <button
          className="w-3 h-3 bg-red-500 rounded-sm hover:bg-red-400 transition-colors flex-shrink-0"
          onClick={(e) => {
            e.stopPropagation();
            // Handle stop
          }}
        />

        {/* Waveform bars */}
        <div className="flex items-center gap-0.5 h-5">
          {[...Array(10)].map((_, i) => {
            const baseHeight = 6;
            const maxHeight = 14;
            const normalizedLevel = Math.min(audioLevel / 15, 1);
            const variation = i * 0.2;
            const time = Date.now() * 0.004;
            const phase = i * 0.4;
            const sineVariation = Math.sin(time + phase) * 0.6;
            const randomVariation = (Math.random() - 0.5) * 1.0;

            const height =
              baseHeight +
              normalizedLevel * (maxHeight - baseHeight) +
              variation +
              sineVariation +
              randomVariation;
            const finalHeight = Math.max(3, Math.min(maxHeight, height));

            return (
              <div
                key={i}
                className="w-0.5 bg-white/70 rounded-full transition-all duration-100"
                style={{ height: `${finalHeight}px` }}
              />
            );
          })}
        </div>
      </div>
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
  const [isListening, setIsListening] = useState(false);

  // Global hotkey listener
  React.useEffect(() => {
    let unlistenHold: (() => void) | undefined;
    let unlistenToggle: (() => void) | undefined;

    const setupListener = async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenHold = await listen<boolean>("fn-hold", (event) => {
          setIsListening(event.payload);
        });
        unlistenToggle = await listen("global-shortcut-pressed", () => {
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
      if (unlistenHold) unlistenHold();
      if (unlistenToggle) unlistenToggle();
    };
  }, []);

  return (
    <div className="h-screen w-screen flex items-center justify-center overflow-hidden bg-transparent">
      <AnimatePresence>
        {isListening && <FloatingPill isActive={true} />}
      </AnimatePresence>
    </div>
  );
}

export default App;
