import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjects } from "../hooks/useProjects";
import { useVmStatus } from "../hooks/useVmStatus";
import { ProjectCard } from "./ProjectCard";
import { CreateProjectDialog } from "./CreateProjectDialog";
import { VmStatusBar } from "./VmStatusBar";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function ProjectDashboard() {
  const { projects, loading, error, createProject, deleteProject } =
    useProjects();
  const vm = useVmStatus();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [authStatus, setAuthStatus] = useState<boolean | null>(null);
  const [authLoading, setAuthLoading] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const authCancelledRef = useRef(false);

  const checkAuth = useCallback(() => {
    if (vm.status !== "Running") return;
    invoke<boolean>("check_auth_status")
      .then(setAuthStatus)
      .catch(() => setAuthStatus(null));
  }, [vm.status]);

  // Poll auth status on mount and when VM becomes ready
  useEffect(() => {
    checkAuth();
  }, [checkAuth]);

  // Re-check auth when window regains focus (e.g. after closing auth window)
  useEffect(() => {
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) checkAuth();
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [checkAuth]);

  const startOAuthFlow = async () => {
    setAuthLoading(true);
    setAuthError(null);
    authCancelledRef.current = false;
    try {
      await invoke("start_oauth_flow");
      checkAuth();
    } catch (err) {
      if (!authCancelledRef.current) {
        setAuthError(String(err));
      }
    } finally {
      setAuthLoading(false);
    }
  };

  const cancelOAuthFlow = async () => {
    authCancelledRef.current = true;
    try {
      await invoke("cancel_oauth_flow");
    } catch (err) {
      console.error("Cancel failed:", err);
    }
    setAuthLoading(false);
  };

  const openTerminal = async (projectId: string) => {
    try {
      await invoke("open_terminal", { projectId });
    } catch (err) {
      console.error("Failed to open terminal:", err);
    }
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-gray-500">
        Loading...
      </div>
    );
  }

  return (
    <div>
      <div className="mb-4">
        <VmStatusBar
          status={vm.status}
          limaVersion={vm.limaVersion}
          actionInProgress={vm.actionInProgress}
          onStart={vm.startVm}
          onStop={vm.stopVm}
          onRetry={vm.refresh}
        />
      </div>

      {vm.status === "Running" && authStatus === false && (
        <div className="mb-4 rounded-md bg-amber-50 border border-amber-200 px-4 py-3">
          <div className="flex items-center justify-between">
            <span className="text-sm text-amber-800">
              {authLoading
                ? "Waiting for sign-in to complete in your browser..."
                : "Claude Code is not signed in. Sign in once to authenticate all projects."}
            </span>
            <div className="ml-4 flex gap-2">
              {authLoading ? (
                <button
                  onClick={cancelOAuthFlow}
                  className="rounded-md bg-gray-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-gray-600"
                >
                  Cancel
                </button>
              ) : (
                <button
                  onClick={startOAuthFlow}
                  className="rounded-md bg-amber-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-amber-700"
                >
                  Sign in to Claude
                </button>
              )}
            </div>
          </div>
          {authError && (
            <p className="mt-2 text-sm text-red-700">{authError}</p>
          )}
        </div>
      )}

      {vm.status === "Running" && authStatus === true && (
        <div className="mb-4 flex items-center justify-between rounded-md bg-green-50 border border-green-200 px-4 py-3">
          <span className="text-sm text-green-800">
            Claude Code: Signed in
          </span>
          <button
            onClick={async () => {
              try {
                await invoke("sign_out");
                checkAuth();
              } catch (err) {
                console.error("Sign out failed:", err);
              }
            }}
            className="ml-4 rounded-md bg-green-700 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-800"
          >
            Sign out
          </button>
        </div>
      )}

      {error && (
        <div className="mb-4 rounded-md bg-red-50 px-4 py-3 text-sm text-red-800">
          {error}
        </div>
      )}

      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-xl font-semibold text-gray-900">Projects</h2>
        <button
          onClick={() => setDialogOpen(true)}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
        >
          + New Project
        </button>
      </div>

      {projects.length === 0 ? (
        <div className="rounded-lg border-2 border-dashed border-gray-300 p-12 text-center">
          <p className="text-gray-500">
            No projects yet. Create one to get started.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {projects.map((project) => (
            <ProjectCard
              key={project.id}
              project={project}
              onDelete={deleteProject}
              onOpen={openTerminal}
            />
          ))}
        </div>
      )}

      <CreateProjectDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        onCreate={createProject}
      />
    </div>
  );
}
