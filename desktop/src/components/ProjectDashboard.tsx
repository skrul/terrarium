import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjects } from "../hooks/useProjects";
import { useVmStatus } from "../hooks/useVmStatus";
import { ProjectCard } from "./ProjectCard";
import { CreateProjectDialog } from "./CreateProjectDialog";
import { VmStatusBar } from "./VmStatusBar";

export function ProjectDashboard() {
  const { projects, loading, error, createProject, deleteProject } =
    useProjects();
  const vm = useVmStatus();
  const [dialogOpen, setDialogOpen] = useState(false);

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
