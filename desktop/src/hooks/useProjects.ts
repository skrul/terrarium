import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Project } from "../types/project";

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (clearError = true) => {
    try {
      const list = await invoke<Project[]>("list_projects");
      setProjects(list);
      if (clearError) setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const createProject = useCallback(
    async (name: string) => {
      setError(null);
      try {
        await invoke("create_project", { name });
        await refresh();
      } catch (e) {
        setError(String(e));
        await refresh(false);
      }
    },
    [refresh],
  );

  const deleteProject = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await invoke("delete_project", { id });
        await refresh();
      } catch (e) {
        setError(String(e));
        await refresh();
      }
    },
    [refresh],
  );

  return { projects, loading, error, createProject, deleteProject, refresh };
}
