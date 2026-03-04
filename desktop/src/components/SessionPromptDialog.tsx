interface Props {
  onContinue: () => void;
  onStartNew: () => void;
}

export function SessionPromptDialog({ onContinue, onStartNew }: Props) {
  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-full max-w-sm rounded-lg bg-white p-6 shadow-xl">
        <h2 className="mb-2 text-lg font-semibold text-gray-900">
          Previous session found
        </h2>
        <p className="mb-5 text-sm text-gray-600">
          A previous Claude Code session exists for this project. Would you like
          to continue where you left off or start fresh?
        </p>
        <div className="flex justify-end gap-2">
          <button
            onClick={onStartNew}
            className="rounded-md px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-100"
          >
            Start new
          </button>
          <button
            autoFocus
            onClick={onContinue}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
          >
            Continue previous
          </button>
        </div>
      </div>
    </div>
  );
}
