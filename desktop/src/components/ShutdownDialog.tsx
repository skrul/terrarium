import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Phase = "stopping" | "timeout";

interface Props {
  open: boolean;
}

export function ShutdownDialog({ open }: Props) {
  const [phase, setPhase] = useState<Phase>("stopping");
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const startedRef = useRef(false);

  const quit = useCallback(async () => {
    await getCurrentWindow().destroy();
  }, []);

  const cleanup = useCallback(() => {
    if (timerRef.current) clearInterval(timerRef.current);
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
  }, []);

  const startStop = useCallback(async () => {
    if (startedRef.current) return;
    startedRef.current = true;

    setPhase("stopping");
    setError(null);
    setElapsed(0);

    timerRef.current = setInterval(() => {
      setElapsed((prev) => prev + 1);
    }, 1000);

    timeoutRef.current = setTimeout(() => {
      setPhase("timeout");
    }, 15_000);

    try {
      await invoke("stop_vm");
      cleanup();
      await quit();
    } catch (e) {
      cleanup();
      setError(String(e));
      setPhase("timeout");
    }
  }, [quit, cleanup]);

  // Start the stop immediately when opened
  useEffect(() => {
    if (open) {
      startStop();
    }
    return cleanup;
  }, [open, startStop, cleanup]);

  const handleKeepWaiting = useCallback(() => {
    setPhase("stopping");
    timeoutRef.current = setTimeout(() => {
      setPhase("timeout");
    }, 15_000);
  }, []);

  const handleForceStopAndQuit = useCallback(async () => {
    setError(null);
    try {
      await invoke("force_stop_vm");
    } catch (e) {
      console.error("Force stop failed:", e);
    }
    await quit();
  }, [quit]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        {phase === "stopping" && (
          <>
            <h2 className="mb-2 text-lg font-semibold text-gray-900">
              Shutting down...
            </h2>
            <p className="mb-4 text-sm text-gray-600">
              Gracefully stopping the Terrarium VM.
            </p>
            <div className="flex items-center gap-3">
              <div className="h-4 w-4 animate-spin rounded-full border-2 border-indigo-600 border-t-transparent" />
              <span className="text-sm text-gray-500">{elapsed}s elapsed</span>
            </div>
          </>
        )}

        {phase === "timeout" && (
          <>
            <h2 className="mb-2 text-lg font-semibold text-gray-900">
              VM is taking a while...
            </h2>
            <p className="mb-2 text-sm text-gray-600">
              The VM has been stopping for {elapsed}s. You can keep waiting or
              force an immediate shutdown.
            </p>
            {error && <p className="mb-4 text-sm text-red-600">{error}</p>}
            <div className="flex flex-col gap-3">
              <button
                onClick={handleKeepWaiting}
                className="w-full rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50"
              >
                Keep Waiting
              </button>
              <button
                onClick={handleForceStopAndQuit}
                className="w-full rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700"
              >
                Force Stop & Quit
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
