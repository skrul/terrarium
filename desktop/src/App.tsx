import { useEffect, useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { VmStatus } from "./types/vm";
import "./App.css";
import { ProjectDashboard } from "./components/ProjectDashboard";
import { ShutdownDialog } from "./components/ShutdownDialog";

function App() {
  const [showShutdown, setShowShutdown] = useState(false);
  const showShutdownRef = useRef(false);

  const handleCloseRequested = useCallback(async () => {
    if (showShutdownRef.current) return;

    // Skip dialog if --keep-running flag was passed
    try {
      const keepRunning = await invoke<boolean>("get_keep_running");
      if (keepRunning) {
        await getCurrentWindow().destroy();
        return;
      }
    } catch {
      // If we can't check, show dialog to be safe
    }

    // Check VM status to decide shutdown behavior
    try {
      const vmStatus = await invoke<VmStatus>("get_vm_status");
      if (
        vmStatus === "Stopped" ||
        vmStatus === "NotCreated" ||
        vmStatus === "NotInstalled"
      ) {
        // VM is not running — just quit
        await getCurrentWindow().destroy();
        return;
      }
      if (vmStatus === "Starting") {
        // VM is still booting — force stop and quit
        try {
          await invoke("force_stop_vm");
        } catch {
          // Best effort
        }
        await getCurrentWindow().destroy();
        return;
      }
    } catch {
      // If we can't check status, show dialog to be safe
    }

    // VM is running — show the graceful shutdown dialog
    showShutdownRef.current = true;
    setShowShutdown(true);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    getCurrentWindow()
      .onCloseRequested(async (event) => {
        event.preventDefault();
        await handleCloseRequested();
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      unlisten?.();
    };
  }, [handleCloseRequested]);

  return (
    <div className="min-h-screen bg-gray-50">
      <header className="border-b border-gray-200 bg-white px-6 py-4">
        <h1 className="text-2xl font-bold text-gray-900">Terrarium</h1>
      </header>
      <main className="mx-auto max-w-5xl px-6 py-8">
        <ProjectDashboard />
      </main>
      <ShutdownDialog open={showShutdown} />
    </div>
  );
}

export default App;
