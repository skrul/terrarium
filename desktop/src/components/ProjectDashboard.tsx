import { useState } from "react";
import { useProjects } from "../hooks/useProjects";
import { useVmStatus } from "../hooks/useVmStatus";
import { ProjectCard } from "./ProjectCard";
import { CreateProjectDialog } from "./CreateProjectDialog";
import { VmStatusBar } from "./VmStatusBar";

export function ProjectDashboard() {
  const { projects, loading, error, createProject, deleteProject, startProject, stopProject } =
    useProjects();
  const vm = useVmStatus();
  const vmReady = vm.status === "Running";
  const [dialogOpen, setDialogOpen] = useState(false);

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
          disabled={!vmReady}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
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
              vmReady={vmReady}
              onDelete={deleteProject}
              onStart={startProject}
              onStop={stopProject}
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
