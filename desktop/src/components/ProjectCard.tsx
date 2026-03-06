import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Project, ProjectStatus } from "../types/project";

const statusColors: Record<ProjectStatus, string> = {
  Creating: "bg-yellow-100 text-yellow-800",
  Ready: "bg-blue-100 text-blue-800",
  Running: "bg-green-100 text-green-800",
  Stopped: "bg-gray-100 text-gray-800",
  Error: "bg-red-100 text-red-800",
};

interface Props {
  project: Project;
  vmReady: boolean;
  onDelete: (id: string) => void;
  onStart: (id: string) => Promise<void>;
  onStop: (id: string) => Promise<void>;
}

export function ProjectCard({ project, vmReady, onDelete, onStart, onStop }: Props) {
  const [actionInProgress, setActionInProgress] = useState<"starting" | "stopping" | null>(null);
  const created = new Date(project.created_at).toLocaleDateString();
  const isCreating = project.status === "Creating";
  const isRunning = project.status === "Running";
  const isStopped = project.status === "Stopped";
  const isError = project.status === "Error";
  const busy = actionInProgress !== null;

  // Show shortened workspace path (~/Terrarium/...)
  const shortPath = project.workspace_path.replace(
    /^\/Users\/[^/]+/,
    "~"
  );

  const openInTerminal = async () => {
    try {
      await invoke("open_in_terminal", { id: project.id });
    } catch (err) {
      console.error("Failed to open terminal:", err);
    }
  };

  const handleStart = async () => {
    setActionInProgress("starting");
    try {
      await onStart(project.id);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleStop = async () => {
    setActionInProgress("stopping");
    try {
      await onStop(project.id);
    } finally {
      setActionInProgress(null);
    }
  };

  const displayStatus = actionInProgress === "starting"
    ? "Starting"
    : actionInProgress === "stopping"
      ? "Stopping"
      : isCreating
        ? "Creating..."
        : project.status;

  const badgeColor = busy ? "bg-yellow-100 text-yellow-800" : statusColors[project.status];

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <div className="mb-3 flex items-start justify-between">
        <h3 className="text-lg font-semibold text-gray-900">{project.name}</h3>
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${badgeColor}`}
        >
          {isRunning && !busy && (
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex h-2 w-2 rounded-full bg-green-500"></span>
            </span>
          )}
          {busy && (
            <span className="h-3 w-3 animate-spin rounded-full border-2 border-yellow-600 border-t-transparent" />
          )}
          {displayStatus}
        </span>
      </div>
      <p className="mb-1 text-sm text-gray-500">Created {created}</p>
      <p className="mb-4 truncate text-xs text-gray-400 font-mono">{shortPath}</p>
      <div className="flex gap-2">
        {(isStopped || isError) && !busy && (
          <button
            onClick={handleStart}
            disabled={!vmReady}
            className="rounded bg-green-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50"
          >
            Start
          </button>
        )}
        {isRunning && !busy && (
          <>
            <button
              onClick={openInTerminal}
              className="rounded bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-700"
            >
              Open Terminal
            </button>
            <button
              onClick={handleStop}
              className="rounded bg-gray-200 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-300"
            >
              Stop
            </button>
          </>
        )}
        <button
          onClick={() => onDelete(project.id)}
          disabled={isCreating || busy}
          className="rounded bg-red-50 px-3 py-1.5 text-sm font-medium text-red-700 hover:bg-red-100 disabled:opacity-50"
        >
          Delete
        </button>
      </div>
    </div>
  );
}
