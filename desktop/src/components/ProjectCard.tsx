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
  onDelete: (id: string) => void;
}

export function ProjectCard({ project, onDelete }: Props) {
  const created = new Date(project.created_at).toLocaleDateString();
  const isCreating = project.status === "Creating";
  const isRunning = project.status === "Running";

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


  return (
    <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <div className="mb-3 flex items-start justify-between">
        <h3 className="text-lg font-semibold text-gray-900">{project.name}</h3>
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${statusColors[project.status]}`}
        >
          {isRunning && (
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex h-2 w-2 rounded-full bg-green-500"></span>
            </span>
          )}
          {isCreating ? "Creating..." : project.status}
        </span>
      </div>
      <p className="mb-1 text-sm text-gray-500">Created {created}</p>
      <p className="mb-4 truncate text-xs text-gray-400 font-mono">{shortPath}</p>
      <div className="flex gap-2">
        <button
          onClick={openInTerminal}
          disabled={!isRunning}
          className="rounded bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          Open Terminal
        </button>
        <button
          onClick={() => onDelete(project.id)}
          disabled={isCreating}
          className="rounded bg-red-50 px-3 py-1.5 text-sm font-medium text-red-700 hover:bg-red-100 disabled:opacity-50"
        >
          Delete
        </button>
      </div>
    </div>
  );
}
