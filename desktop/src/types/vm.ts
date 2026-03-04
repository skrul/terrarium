export type VmStatus =
  | "NotInstalled"
  | "NotCreated"
  | "Stopped"
  | "Starting"
  | "Running"
  | { Error: { message: string } };

export interface RuntimeStatus {
  vm_status: VmStatus;
  lima_version: string | null;
}

export function vmStatusLabel(status: VmStatus): string {
  if (typeof status === "string") return status;
  return `Error: ${status.Error.message}`;
}

export function isVmError(status: VmStatus): status is { Error: { message: string } } {
  return typeof status === "object" && "Error" in status;
}
