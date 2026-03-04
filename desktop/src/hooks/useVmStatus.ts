import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RuntimeStatus, VmStatus } from "../types/vm";

const POLL_INTERVAL = 3000;

export function useVmStatus() {
  const [status, setStatus] = useState<VmStatus>("NotCreated");
  const [limaVersion, setLimaVersion] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionInProgress, setActionInProgress] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<RuntimeStatus>("get_runtime_status");
      setStatus(result.vm_status);
      setLimaVersion(result.lima_version);
    } catch {
      setStatus({ Error: { message: "Failed to get VM status" } });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(refresh, POLL_INTERVAL);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refresh]);

  const startVm = useCallback(async () => {
    setActionInProgress(true);
    setStatus("Starting");
    try {
      await invoke("start_vm");
      await refresh();
    } catch (e) {
      setStatus({ Error: { message: String(e) } });
    } finally {
      setActionInProgress(false);
    }
  }, [refresh]);

  const stopVm = useCallback(async () => {
    setActionInProgress(true);
    try {
      await invoke("stop_vm");
      await refresh();
    } catch (e) {
      setStatus({ Error: { message: String(e) } });
    } finally {
      setActionInProgress(false);
    }
  }, [refresh]);

  return { status, limaVersion, loading, actionInProgress, startVm, stopVm, refresh };
}
