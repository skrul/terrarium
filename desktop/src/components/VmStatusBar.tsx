import { VmStatus, isVmError } from "../types/vm";

interface Props {
  status: VmStatus;
  limaVersion: string | null;
  onRetry: () => void;
}

export function VmStatusBar({ status, limaVersion, onRetry }: Props) {
  if (status === "NotInstalled") {
    return (
      <div className="rounded-md bg-red-50 px-4 py-3 text-sm text-red-800">
        <span className="font-medium">Lima is not installed.</span>{" "}
        Install it with:{" "}
        <code className="rounded bg-red-100 px-1.5 py-0.5 font-mono text-xs">
          brew install lima
        </code>{" "}
        then restart Terrarium.
      </div>
    );
  }

  if (isVmError(status)) {
    return (
      <div className="flex items-center justify-between rounded-md bg-red-50 px-4 py-2 text-sm text-red-800">
        <span>
          <span className="font-medium">VM Error:</span> {status.Error.message}
        </span>
        <button
          onClick={onRetry}
          className="rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2 rounded-md bg-gray-50 px-4 py-2 text-sm text-gray-600">
      <StatusDot status={status} />
      <span>{statusText(status)}</span>
      {limaVersion && (
        <span className="text-xs text-gray-400">({limaVersion})</span>
      )}
    </div>
  );
}

function StatusDot({ status }: { status: VmStatus }) {
  let color = "bg-gray-400";
  if (status === "Running") color = "bg-green-500";
  else if (status === "Stopped") color = "bg-yellow-500";
  else if (status === "Starting") color = "bg-yellow-400 animate-pulse";

  return <span className={`inline-block h-2 w-2 rounded-full ${color}`} />;
}

function statusText(status: VmStatus): string {
  switch (status) {
    case "NotCreated":
      return "VM will be created on first project";
    case "Stopped":
      return "VM starting...";
    case "Starting":
      return "VM starting...";
    case "Running":
      return "VM running";
    default:
      return "";
  }
}
