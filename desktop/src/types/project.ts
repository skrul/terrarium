export type ProjectStatus = "Creating" | "Ready" | "Running" | "Stopped" | "Error";

export interface Project {
  id: string;
  name: string;
  status: ProjectStatus;
  created_at: string;
}
