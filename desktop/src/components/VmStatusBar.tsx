import { VmStatus, isVmError } from "../types/vm";

interface Props {
  status: VmStatus;
  limaVersion: string | null;
  actionInProgress: boolean;
  onStart: () => void;
  onStop: () => void;
  onRetry: () => void;
}

export function VmStatusBar({
  status,
  limaVersion,
  actionInProgress,
  onStart,
  onStop,
  onRetry,
}: Props) {
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
          disabled={actionInProgress}
          className="rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700 disabled:opacity-50"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between rounded-md bg-gray-50 px-4 py-2 text-sm text-gray-600">
      <div className="flex items-center gap-2">
        <StatusDot status={status} />
        <span>{statusText(status)}</span>
        {limaVersion && (
          <span className="text-xs text-gray-400">({limaVersion})</span>
        )}
      </div>
      <div>
        {status === "Stopped" && (
          <button
            onClick={onStart}
            disabled={actionInProgress}
            className="rounded bg-green-600 px-3 py-1 text-xs font-medium text-white hover:bg-green-700 disabled:opacity-50"
          >
            Start VM
          </button>
        )}
        {status === "Starting" && (
          <span className="text-xs text-gray-400">Starting...</span>
        )}
        {status === "Running" && (
          <button
            onClick={onStop}
            disabled={actionInProgress}
            className="rounded bg-gray-200 px-3 py-1 text-xs font-medium text-gray-700 hover:bg-gray-300 disabled:opacity-50"
          >
            Stop VM
          </button>
        )}
      </div>
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
      return "VM stopped";
    case "Starting":
      return "VM starting...";
    case "Running":
      return "VM running";
    default:
      return "";
  }
}
