import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SessionPromptDialog } from "./SessionPromptDialog";
import "@xterm/xterm/css/xterm.css";

interface Props {
  projectId: string;
}

export function TerminalView({ projectId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // null = still checking, true = show dialog, false = skip dialog
  const [showSessionPrompt, setShowSessionPrompt] = useState<boolean | null>(
    null
  );
  // undefined = no choice yet
  const [continueSession, setContinueSession] = useState<boolean | undefined>(
    undefined
  );

  // Phase 1: Check for existing Claude Code sessions
  useEffect(() => {
    invoke<boolean>("check_claude_sessions", { projectId })
      .then((hasSessions) => {
        if (hasSessions) {
          setShowSessionPrompt(true);
        } else {
          setShowSessionPrompt(false);
          setContinueSession(false);
        }
      })
      .catch(() => {
        // On error, just start fresh
        setShowSessionPrompt(false);
        setContinueSession(false);
      });
  }, [projectId]);

  // Phase 2: Initialize terminal once the user has made a choice
  useEffect(() => {
    if (continueSession === undefined) return;
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: "Menlo, Monaco, 'Courier New', monospace",
      theme: {
        background: "#1e1e1e",
        foreground: "#d4d4d4",
      },
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);
    fitAddon.fit();

    termRef.current = term;
    fitAddonRef.current = fitAddon;

    const cols = term.cols;
    const rows = term.rows;

    // Listen for terminal output (base64-encoded)
    const unlistenOutput = listen<string>("terminal-output", (event) => {
      const bytes = Uint8Array.from(atob(event.payload), (c) =>
        c.charCodeAt(0)
      );
      term.write(bytes);
    });

    // Listen for session exit
    const unlistenExit = listen<string>("terminal-exit", () => {
      term.write(
        "\r\n\x1b[90m[Session ended. Close this window or click Open to reconnect.]\x1b[0m\r\n"
      );
    });

    // Send user input to the PTY (base64-encoded)
    const onData = term.onData((data) => {
      const encoded = btoa(data);
      invoke("write_terminal", {
        sessionId: projectId,
        data: encoded,
      }).catch(() => {});
    });

    // Start the PTY session (listeners are already set up above)
    invoke("start_terminal", {
      projectId,
      continueSession,
      cols,
      rows,
    }).catch((err) => {
      term.write(`\r\n\x1b[31mFailed to start terminal: ${err}\x1b[0m\r\n`);
    });

    // Resize handling
    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      invoke("resize_terminal", {
        sessionId: projectId,
        cols: term.cols,
        rows: term.rows,
      }).catch(() => {});
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      resizeObserver.disconnect();
      onData.dispose();
      unlistenOutput.then((fn) => fn());
      unlistenExit.then((fn) => fn());
      term.dispose();
    };
  }, [projectId, continueSession]);

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        backgroundColor: "#1e1e1e",
        overflow: "hidden",
        position: "relative",
      }}
    >
      <div ref={containerRef} style={{ width: "100%", height: "100%" }} />
      {showSessionPrompt && (
        <SessionPromptDialog
          onContinue={() => {
            setShowSessionPrompt(false);
            setContinueSession(true);
          }}
          onStartNew={() => {
            setShowSessionPrompt(false);
            setContinueSession(false);
          }}
        />
      )}
    </div>
  );
}
